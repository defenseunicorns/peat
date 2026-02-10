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

# ── Yard Optimizer tool definitions ────────────────────────────────────────

YARD_OPTIMIZER_TOOLS = [
    ToolShim(
        name="publish_optimization_plan",
        description=(
            "Publish a yard optimization plan with block allocation strategy "
            "and utilization metrics."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "zone": {"type": "string", "description": "Yard zone identifier"},
                "block_count": {"type": "integer", "description": "Number of blocks in zone"},
                "total_capacity_teu": {"type": "integer", "description": "Total TEU capacity"},
                "total_used_teu": {"type": "integer", "description": "Currently used TEU"},
                "utilization": {"type": "number", "description": "Utilization ratio (0-1)"},
                "strategy": {
                    "type": "string",
                    "enum": ["proximity", "balanced", "overflow"],
                    "description": "Current allocation strategy",
                },
                "summary": {"type": "string", "description": "Human-readable summary"},
            },
            "required": ["zone", "utilization", "strategy", "summary"],
        },
    ),
    ToolShim(
        name="allocate_block",
        description=(
            "Assign an inbound container to an optimal yard block based on "
            "utilization, proximity, and type compatibility."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {"type": "string", "description": "Container to allocate"},
                "target_block": {"type": "string", "description": "Target yard block ID"},
                "container_type": {
                    "type": "string",
                    "enum": ["standard", "reefer", "hazmat"],
                    "description": "Container type for compatibility check",
                },
                "reason": {"type": "string", "description": "Allocation rationale"},
            },
            "required": ["container_id", "target_block", "reason"],
        },
    ),
    ToolShim(
        name="match_backhaul",
        description=(
            "Match an idle tractor with a nearby pickup to minimize empty return trips."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "tractor_id": {"type": "string", "description": "Tractor to assign backhaul"},
                "current_block": {"type": "string", "description": "Tractor's current block"},
                "pickup_block": {"type": "string", "description": "Block with pending pickup"},
                "reason": {"type": "string", "description": "Backhaul match rationale"},
            },
            "required": ["tractor_id", "current_block", "pickup_block", "reason"],
        },
    ),
    ToolShim(
        name="emit_yard_optimization_event",
        description=(
            "Emit a yard optimization event up the HIVE hierarchy."
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

# ── Gate Flow AI tool definitions ─────────────────────────────────────────

GATE_FLOW_TOOLS = [
    ToolShim(
        name="publish_gate_flow_plan",
        description=(
            "Publish a gate flow plan with queue metrics, appointment status, "
            "and throttle state."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "queue_length": {"type": "integer", "description": "Current truck queue length"},
                "pending_appointments": {"type": "integer", "description": "Pending appointment count"},
                "avg_wait_minutes": {"type": "number", "description": "Average truck wait time in minutes"},
                "yard_ready_count": {"type": "integer", "description": "Containers ready for pickup"},
                "throttle_active": {"type": "boolean", "description": "Whether appointment throttling is active"},
                "summary": {"type": "string", "description": "Human-readable summary"},
            },
            "required": ["queue_length", "throttle_active", "summary"],
        },
    ),
    ToolShim(
        name="throttle_appointments",
        description=(
            "Activate or adjust appointment throttling to manage truck queue depth."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["reduce_rate", "pause", "resume"],
                    "description": "Throttle action",
                },
                "current_queue": {"type": "integer", "description": "Current queue length"},
                "target_queue": {"type": "integer", "description": "Target queue length"},
                "reason": {"type": "string", "description": "Throttle rationale"},
            },
            "required": ["action", "reason"],
        },
    ),
    ToolShim(
        name="schedule_pickup",
        description=(
            "Schedule a truck pickup by matching a waiting truck with a yard-ready container."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "truck_id": {"type": "string", "description": "Truck identifier"},
                "container_id": {"type": "string", "description": "Container to pick up"},
                "gate_lane": {"type": "string", "description": "Assigned gate lane"},
                "priority": {
                    "type": "string",
                    "enum": ["LOW", "NORMAL", "HIGH", "URGENT"],
                    "description": "Pickup priority",
                },
            },
            "required": ["truck_id", "container_id", "gate_lane"],
        },
    ),
    ToolShim(
        name="schedule_rail_loading",
        description=(
            "Schedule a batch of containers for rail loading in a time window."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "rail_track": {"type": "string", "description": "Rail track identifier"},
                "containers": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Container IDs to load",
                },
                "window_start": {"type": "string", "description": "Loading window start time"},
                "estimated_duration_min": {"type": "number", "description": "Estimated loading duration in minutes"},
            },
            "required": ["rail_track", "containers"],
        },
    ),
    ToolShim(
        name="emit_gate_flow_event",
        description=(
            "Emit a gate flow event up the HIVE hierarchy."
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


# ── Gate Scanner tool definitions ────────────────────────────────────────────

GATE_SCANNER_TOOLS = [
    ToolShim(
        name="scan_container",
        description=(
            "Perform optical and weight scan of a container at the gate lane. "
            "Reports measured weight, damage status, and OCR container number."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID being scanned",
                },
                "measured_weight_tons": {
                    "type": "number",
                    "description": "Measured weight in metric tons",
                },
                "declared_weight_tons": {
                    "type": "number",
                    "description": "Declared weight from shipping documents",
                },
                "weight_within_tolerance": {
                    "type": "boolean",
                    "description": "Whether weight is within 5% SOLAS VGM tolerance",
                },
                "damage_detected": {
                    "type": "boolean",
                    "description": "Whether structural damage was detected",
                },
                "confidence": {
                    "type": "number",
                    "description": "Scan confidence (0.0-1.0)",
                },
            },
            "required": ["container_id"],
        },
    ),
    ToolShim(
        name="report_equipment_status",
        description=(
            "Report gate scanner operational status (OPERATIONAL, DEGRADED, FAILED, FLAGGED)."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["OPERATIONAL", "DEGRADED", "FAILED", "FLAGGED"],
                    "description": "Equipment status",
                },
                "details": {
                    "type": "string",
                    "description": "Status details",
                },
            },
            "required": ["status", "details"],
        },
    ),
]

# ── Gate Worker tool definitions ─────────────────────────────────────────────

GATE_WORKER_TOOLS = [
    ToolShim(
        name="verify_documents",
        description=(
            "Check truck/container documents (bill of lading, customs clearance) "
            "against expected manifest."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID to verify",
                },
                "customs_cleared": {
                    "type": "boolean",
                    "description": "Whether customs clearance is confirmed",
                },
                "bill_of_lading": {
                    "type": "boolean",
                    "description": "Whether bill of lading matches",
                },
                "documents_valid": {
                    "type": "boolean",
                    "description": "Overall document validity",
                },
            },
            "required": ["container_id", "documents_valid"],
        },
    ),
    ToolShim(
        name="process_truck",
        description=(
            "Complete truck processing — release to yard or reject with reason code."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID on the truck",
                },
                "action": {
                    "type": "string",
                    "enum": ["release", "reject"],
                    "description": "Release to yard or reject",
                },
                "gate_lane": {
                    "type": "string",
                    "description": "Gate lane ID (e.g., gate-a-1)",
                },
                "reasons": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Rejection reason codes (if rejecting)",
                },
            },
            "required": ["container_id", "action"],
        },
    ),
    ToolShim(
        name="inspect_seal",
        description=(
            "Record seal inspection result — seal integrity and number verification."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID",
                },
                "seal_intact": {
                    "type": "boolean",
                    "description": "Whether the seal is physically intact",
                },
                "seal_number_match": {
                    "type": "boolean",
                    "description": "Whether seal number matches documentation",
                },
            },
            "required": ["container_id", "seal_intact", "seal_number_match"],
        },
    ),
    ToolShim(
        name="report_equipment_status",
        description=(
            "Report gate lane operational status."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["OPERATIONAL", "DEGRADED", "FAILED", "MAINTENANCE"],
                    "description": "Lane status",
                },
                "details": {
                    "type": "string",
                    "description": "Status details",
                },
            },
            "required": ["status", "details"],
        },
    ),
]

# ── TOC tool definitions ──────────────────────────────────────────────────

TOC_TOOLS = [
    ToolShim(
        name="update_terminal_summary",
        description=(
            "Update the terminal-level summary with aggregated metrics from all zones. "
            "Computes total rate, zone statuses, and terminal-wide completion ETA."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "total_moves_per_hour": {
                    "type": "number",
                    "description": "Aggregate moves/hour across all berths",
                },
                "berth_statuses": {
                    "type": "object",
                    "description": "Mapping of berth_id → status",
                },
                "completion_eta_minutes": {
                    "type": "number",
                    "description": "Estimated minutes to complete all terminal operations",
                },
                "total_moves_remaining": {
                    "type": "number",
                    "description": "Total container moves remaining across all berths",
                },
                "summary": {
                    "type": "string",
                    "description": "Brief human-readable terminal summary",
                },
            },
            "required": ["total_moves_per_hour", "berth_statuses", "completion_eta_minutes", "summary"],
        },
    ),
    ToolShim(
        name="authorize_resource_transfer",
        description=(
            "Authorize a large-scale resource transfer between zones. "
            "Emits resource_transfer_authorized event with HIGH priority."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "resource_type": {
                    "type": "string",
                    "description": "Type of resource (tractor, crew, equipment)",
                },
                "from_zone": {
                    "type": "string",
                    "description": "Source zone ID",
                },
                "to_zone": {
                    "type": "string",
                    "description": "Destination zone ID",
                },
                "reason": {
                    "type": "string",
                    "description": "Reason for the transfer",
                },
            },
            "required": ["resource_type", "from_zone", "to_zone", "reason"],
        },
    ),
    ToolShim(
        name="emit_terminal_alert",
        description=(
            "Emit a terminal-wide alert (cross-zone gap, cascading failure, zone escalation). "
            "Alerts propagate to port authority with IMMEDIATE_PROPAGATE policy."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "alert_type": {
                    "type": "string",
                    "description": "Alert type (cross_zone_gap, cascading_failure, zone_gap_escalation)",
                },
                "details": {
                    "type": "string",
                    "description": "Alert details",
                },
                "severity": {
                    "type": "string",
                    "enum": ["NORMAL", "HIGH", "CRITICAL"],
                    "description": "Alert severity",
                },
                "affected_zones": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "List of affected zone IDs",
                },
            },
            "required": ["alert_type", "details", "severity"],
        },
    ),
    ToolShim(
        name="adjust_zone_priority",
        description=(
            "Adjust the operational priority of a zone based on terminal constraints. "
            "Higher-priority zones receive preferential resource allocation."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "zone_id": {
                    "type": "string",
                    "description": "Zone ID to adjust priority for",
                },
                "priority_level": {
                    "type": "string",
                    "enum": ["LOW", "NORMAL", "HIGH", "CRITICAL"],
                    "description": "New priority level for the zone",
                },
                "reason": {
                    "type": "string",
                    "description": "Reason for the priority adjustment",
                },
            },
            "required": ["zone_id", "priority_level", "reason"],
        },
    ),
]

# ── Yard Manager tool definitions ─────────────────────────────────────────

YARD_MANAGER_TOOLS = [
    ToolShim(
        name="update_yard_summary",
        description=(
            "Publish consolidated yard zone status aggregating all subordinate block summaries. "
            "Computes total TEU capacity, utilization, and reefer/hazmat status."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "zone": {
                    "type": "string",
                    "description": "Yard zone identifier",
                },
                "block_count": {
                    "type": "number",
                    "description": "Number of yard blocks in this zone",
                },
                "total_capacity_teu": {
                    "type": "number",
                    "description": "Total TEU capacity across all blocks",
                },
                "total_used_teu": {
                    "type": "number",
                    "description": "Total TEU currently used",
                },
                "utilization": {
                    "type": "number",
                    "description": "Utilization ratio (0.0-1.0)",
                },
                "summary": {
                    "type": "string",
                    "description": "Brief human-readable yard summary",
                },
            },
            "required": ["zone", "utilization", "summary"],
        },
    ),
    ToolShim(
        name="assign_yard_block",
        description=(
            "Direct a container or crane to a specific yard block. "
            "Also supports crane rebalancing between blocks."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID to assign (optional for rebalance)",
                },
                "block_id": {
                    "type": "string",
                    "description": "Target yard block ID",
                },
                "action": {
                    "type": "string",
                    "description": "Action type (assign or rebalance_crane)",
                },
                "from_block": {
                    "type": "string",
                    "description": "Source block for crane rebalance",
                },
                "to_block": {
                    "type": "string",
                    "description": "Target block for crane rebalance",
                },
            },
        },
    ),
    ToolShim(
        name="route_tractor",
        description=(
            "Set a tractor's routing path through the yard to a target block."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "tractor_id": {
                    "type": "string",
                    "description": "Tractor entity ID to route",
                },
                "target_block": {
                    "type": "string",
                    "description": "Destination yard block ID",
                },
                "container_id": {
                    "type": "string",
                    "description": "Container being transported (optional)",
                },
            },
            "required": ["tractor_id", "target_block"],
        },
    ),
    ToolShim(
        name="report_congestion",
        description=(
            "Flag a congestion event for mitigation or TOC escalation."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "block_id": {
                    "type": "string",
                    "description": "Congested block ID",
                },
                "queue_depth": {
                    "type": "number",
                    "description": "Number of tractors queued",
                },
                "zone": {
                    "type": "string",
                    "description": "Yard zone ID",
                },
                "escalate_to": {
                    "type": "string",
                    "description": "Escalation target (e.g., 'toc')",
                },
                "reasons": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Escalation reasons",
                },
            },
            "required": ["zone"],
        },
    ),
]

# ── Stacking Crane tool definitions ───────────────────────────────────────

STACKING_CRANE_TOOLS = [
    ToolShim(
        name="stack_container",
        description=(
            "Place a container at the assigned row/bay/tier slot in the yard block."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "crane_id": {
                    "type": "string",
                    "description": "Stacking crane entity ID",
                },
                "container_id": {
                    "type": "string",
                    "description": "Container ID to stack",
                },
                "row": {
                    "type": "number",
                    "description": "Target row in yard block",
                },
                "bay": {
                    "type": "number",
                    "description": "Target bay in yard block",
                },
                "tier": {
                    "type": "number",
                    "description": "Target tier (stack height)",
                },
                "yard_block": {
                    "type": "string",
                    "description": "Yard block ID",
                },
            },
            "required": ["container_id", "row", "bay", "tier", "yard_block"],
        },
    ),
    ToolShim(
        name="retrieve_container",
        description=(
            "Pick up a container from a yard slot for outbound transfer."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "crane_id": {
                    "type": "string",
                    "description": "Stacking crane entity ID",
                },
                "container_id": {
                    "type": "string",
                    "description": "Container ID to retrieve",
                },
                "row": {
                    "type": "number",
                    "description": "Source row in yard block",
                },
                "bay": {
                    "type": "number",
                    "description": "Source bay in yard block",
                },
                "tier": {
                    "type": "number",
                    "description": "Source tier",
                },
                "yard_block": {
                    "type": "string",
                    "description": "Yard block ID",
                },
            },
            "required": ["container_id", "row", "bay", "tier", "yard_block"],
        },
    ),
    ToolShim(
        name="report_position",
        description=(
            "Broadcast crane position, task status, and subsystem health."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "crane_id": {
                    "type": "string",
                    "description": "Stacking crane entity ID",
                },
                "status": {
                    "type": "string",
                    "description": "Current crane status (idle, received, delivered, fault)",
                },
                "position": {
                    "type": "object",
                    "description": "Current position {row, bay}",
                },
                "yard_block": {
                    "type": "string",
                    "description": "Assigned yard block ID",
                },
                "container_id": {
                    "type": "string",
                    "description": "Container currently being handled (optional)",
                },
                "fault": {
                    "type": "string",
                    "description": "Fault description if any (optional)",
                },
                "load_kg": {
                    "type": "number",
                    "description": "Current hoist load in kg (optional)",
                },
            },
            "required": ["crane_id"],
        },
    ),
]

# ── BridgeAPI ────────────────────────────────────────────────────────────────

class BridgeAPI:
    """
    Per-agent interface to a shared HiveStateStore.

    Duck-types the MCP ClientSession interface so AgentLoop can use it
    transparently instead of a real MCP connection.

    Each BridgeAPI is scoped to a specific berth via ``hold_id`` and
    ``berth_id``. Entity state and events are partitioned by berth—agents
    only see state from their own berth unless explicitly accessing
    cross-berth resources (e.g., yard tractors via berth_manager).
    """

    def __init__(
        self,
        store: HiveStateStore,
        node_id: str,
        role: str = "crane",
        hold_id: str = "hold-3",
        berth_id: str = "berth-5",
    ):
        self.store = store
        self.node_id = node_id
        self.role = role
        self.hold_id = hold_id
        self.berth_id = berth_id
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

        elif uri == "hive://tasking":
            if self.role == "toc":
                text = self._read_toc_tasking()
            elif self.role == "yard_manager":
                text = self._read_yard_manager_tasking()
            elif self.role == "stacking_crane":
                text = self._read_stacking_crane_tasking()
            elif self.role == "aggregator":
                text = self._read_aggregator_tasking()
            elif self.role == "berth_manager":
                text = self._read_berth_manager_tasking()
            elif self.role == "operator":
                text = self._read_operator_tasking()
            elif self.role == "tractor":
                text = self._read_tractor_tasking()
            elif self.role == "scheduler":
                text = self._read_scheduler_tasking()
            elif self.role == "yard_optimizer":
                text = self._read_yard_optimizer_tasking()
            elif self.role == "gate_flow":
                text = self._read_gate_flow_tasking()
            elif self.role == "sensor":
                text = self._read_sensor_tasking()
            elif self.role in ("gate_scanner", "rfid_reader"):
                text = self._read_gate_scanner_tasking()
            elif self.role == "gate_worker":
                text = self._read_gate_worker_tasking()
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
        tasking = {
            "directive": "PROCESS_CONTAINERS",
            "assignment": entity.fields.get("assignment", {}) if entity else {},
            "containers_remaining": (
                queue.fields.get("total_containers", 0) - queue.fields.get("completed_count", 0)
                if queue else 0
            ),
            "target_rate": 35,
            "priority": "NORMAL",
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_aggregator_tasking(self) -> str:
        team = self._get_team_doc()
        queue = self._get_queue_doc()
        tasking = {
            "directive": "AGGREGATE_HOLD_STATUS",
            "hold_id": self.hold_id,
            "team_size": len(team.fields.get("team_members", {})) if team else 0,
            "moves_completed": team.fields.get("moves_completed", 0) if team else 0,
            "moves_remaining": team.fields.get("moves_remaining", 0) if team else 0,
            "gap_count": len(team.fields.get("gap_analysis", [])) if team else 0,
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
        tasking = {
            "directive": "TRANSPORT_CONTAINERS",
            "position": entity.fields.get("position", {}) if entity else {},
            "battery_pct": entity.get_field("equipment_health.battery_pct", 100) if entity else 100,
            "pending_transport_jobs": pending_jobs[:3],
            "trips_completed": entity.get_field("metrics.trips_completed", 0) if entity else 0,
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_scheduler_tasking(self) -> str:
        team = self._get_team_doc()
        queue = self._get_queue_doc()
        members = team.fields.get("team_members", {}) if team else {}
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

    def _read_gate_scanner_tasking(self) -> str:
        entity = self._get_entity_doc()
        tasking = {
            "directive": "SCAN_CONTAINERS",
            "current_truck": entity.fields.get("current_truck") if entity else None,
            "calibration": entity.fields.get("calibration", {}) if entity else {},
            "scans_completed": entity.get_field("metrics.scans_completed", 0) if entity else 0,
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_gate_worker_tasking(self) -> str:
        entity = self._get_entity_doc()
        tasking = {
            "directive": "PROCESS_TRUCKS",
            "current_truck": entity.fields.get("current_truck") if entity else None,
            "gate_lane": entity.get_field("gate_lane", self.hold_id) if entity else self.hold_id,
            "trucks_processed": entity.get_field("metrics.trucks_processed", 0) if entity else 0,
            "scanner_results": entity.fields.get("scanner_results", {}) if entity else {},
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_toc_tasking(self) -> str:
        # Aggregate all zone summaries into terminal-level context.
        # TOC sees ALL zones — no berth filtering.
        berth_summaries = []
        total_remaining = 0
        for doc_id, doc in self.store._collections.get("team_summaries", {}).items():
            fields = doc.fields
            hold_id = fields.get("hold_id", doc_id.replace("team_", ""))
            moves_per_hour = fields.get("moves_per_hour", 0)
            status = fields.get("status", "UNKNOWN")
            gap_count = len(fields.get("gap_analysis", []))
            remaining = fields.get("moves_remaining", 0)
            total_remaining += remaining
            berth_summaries.append({
                "berth_id": hold_id,
                "moves_per_hour": moves_per_hour,
                "status": status,
                "gap_count": gap_count,
                "moves_remaining": remaining,
                "moves_completed": fields.get("moves_completed", 0),
            })
        tasking = {
            "directive": "AGGREGATE_TERMINAL_STATUS",
            "zone_count": len(berth_summaries),
            "berth_summaries": berth_summaries,
            "total_terminal_moves_remaining": total_remaining,
            "priority": "NORMAL",
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_yard_manager_tasking(self) -> str:
        entity = self._get_entity_doc()
        tasking = {
            "directive": "COORDINATE_YARD",
            "zone": entity.get_field("assignment.zone", "yard-north") if entity else "yard-north",
            "block_summaries": [],
            "pending_tractors": [],
            "congestion_events": [],
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_stacking_crane_tasking(self) -> str:
        entity = self._get_entity_doc()
        tasking = {
            "directive": "STACK_RETRIEVE_CONTAINERS",
            "crane_id": self.node_id,
            "yard_block": entity.get_field("assignment.yard_block", "YB-A") if entity else "YB-A",
            "position": entity.fields.get("position", {"row": 0, "bay": 0}) if entity else {"row": 0, "bay": 0},
            "hoist_load_kg": entity.get_field("hoist_load_kg", 0.0) if entity else 0.0,
            "current_task": entity.fields.get("current_task") if entity else None,
            "task_queue": [],
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_yard_optimizer_tasking(self) -> str:
        entity = self._get_entity_doc()
        # Gather yard block summaries for optimization decisions
        block_summaries = []
        for doc_id, doc in self.store._collections.get("node_states", {}).items():
            if doc.fields.get("entity_type") == "yard_block":
                block_summaries.append({
                    "block_id": doc.fields.get("block_id", doc_id),
                    "capacity_teu": doc.fields.get("capacity_teu", 200),
                    "used_teu": doc.fields.get("used_teu", 0),
                    "utilization": doc.fields.get("used_teu", 0) / max(doc.fields.get("capacity_teu", 200), 1),
                    "queue_depth": doc.fields.get("queue_depth", 0),
                })
        # Gather tractor positions for backhaul matching
        tractor_positions = []
        for doc_id, doc in self.store._collections.get("node_states", {}).items():
            if doc.fields.get("entity_type") == "yard_tractor":
                tractor_positions.append({
                    "tractor_id": doc.fields.get("node_id", doc_id),
                    "block": doc.fields.get("position", {}).get("block", "unknown"),
                    "status": doc.fields.get("operational_status", "IDLE"),
                })
        tasking = {
            "directive": "OPTIMIZE_YARD",
            "zone": entity.get_field("assignment.zone", "yard-north") if entity else "yard-north",
            "block_summaries": block_summaries,
            "pending_allocations": [],
            "tractor_positions": tractor_positions,
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_gate_flow_tasking(self) -> str:
        entity = self._get_entity_doc()
        # Gather gate queue and yard readiness for flow decisions
        truck_queue = []
        yard_ready = []
        appointments = []
        for doc_id, doc in self.store._collections.get("node_states", {}).items():
            if doc.fields.get("entity_type") in ("gate_scanner", "gate_worker"):
                truck = doc.fields.get("current_truck")
                if truck:
                    truck_queue.append(truck)
        tasking = {
            "directive": "OPTIMIZE_GATE_FLOW",
            "zone": entity.get_field("assignment.zone", "gate-complex") if entity else "gate-complex",
            "truck_queue": truck_queue,
            "appointments": appointments,
            "yard_ready_containers": yard_ready,
            "rail_schedule": [],
            "avg_wait_minutes": 0,
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_berth_manager_tasking(self) -> str:
        # Aggregate hold docs from team_summaries into berth-level context.
        # Each berth manager only sees the hold(s) belonging to its berth.
        hold_summaries = []
        total_remaining = 0
        for doc_id, doc in self.store._collections.get("team_summaries", {}).items():
            fields = doc.fields
            hold_id = fields.get("hold_id", doc_id.replace("team_", ""))
            # Filter: berth manager only sees its own berth's holds
            if self.hold_id != hold_id and self.hold_id not in hold_id:
                # Allow if hold_id matches the berth manager's hold_id
                # (single-berth mode) or contains the berth prefix (multi-berth)
                if not hold_id.startswith(self.hold_id.replace("hold-", "hold-")):
                    continue
            moves_per_hour = fields.get("moves_per_hour", 0)
            status = fields.get("status", "UNKNOWN")
            gap_count = len(fields.get("gap_analysis", []))
            remaining = fields.get("moves_remaining", 0)
            total_remaining += remaining
            hold_summaries.append({
                "hold_id": hold_id,
                "moves_per_hour": moves_per_hour,
                "status": status,
                "gap_count": gap_count,
                "moves_remaining": remaining,
                "moves_completed": fields.get("moves_completed", 0),
            })
        tasking = {
            "directive": "AGGREGATE_BERTH_STATUS",
            "berth_id": self.berth_id,
            "hold_count": len(hold_summaries),
            "hold_summaries": hold_summaries,
            "total_moves_remaining": total_remaining,
            "priority": "NORMAL",
        }
        return json.dumps(tasking, indent=2, default=str)

    async def list_tools(self) -> ListToolsResultShim:
        """Return tools appropriate for this agent's role."""
        if self.role == "toc":
            return ListToolsResultShim(tools=list(TOC_TOOLS))
        elif self.role == "yard_manager":
            return ListToolsResultShim(tools=list(YARD_MANAGER_TOOLS))
        elif self.role == "stacking_crane":
            return ListToolsResultShim(tools=list(STACKING_CRANE_TOOLS))
        elif self.role == "aggregator":
            return ListToolsResultShim(tools=list(AGGREGATOR_TOOLS))
        elif self.role == "berth_manager":
            return ListToolsResultShim(tools=list(BERTH_MANAGER_TOOLS))
        elif self.role == "operator":
            return ListToolsResultShim(tools=list(OPERATOR_TOOLS))
        elif self.role == "tractor":
            return ListToolsResultShim(tools=list(TRACTOR_TOOLS))
        elif self.role == "scheduler":
            return ListToolsResultShim(tools=list(SCHEDULER_TOOLS))
        elif self.role == "yard_optimizer":
            return ListToolsResultShim(tools=list(YARD_OPTIMIZER_TOOLS))
        elif self.role == "gate_flow":
            return ListToolsResultShim(tools=list(GATE_FLOW_TOOLS))
        elif self.role == "sensor":
            return ListToolsResultShim(tools=list(SENSOR_TOOLS))
        elif self.role in ("gate_scanner", "rfid_reader"):
            return ListToolsResultShim(tools=list(GATE_SCANNER_TOOLS))
        elif self.role == "gate_worker":
            return ListToolsResultShim(tools=list(GATE_WORKER_TOOLS))
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
            # Tractor tools
            "transport_container": self._tool_transport_container,
            "report_position": self._tool_report_position,
            "request_charge": self._tool_request_charge,
            # Scheduler tools
            "rebalance_assignments": self._tool_rebalance_assignments,
            "update_priority_queue": self._tool_update_priority_queue,
            "dispatch_resource": self._tool_dispatch_resource,
            "emit_schedule_event": self._tool_emit_schedule_event,
            # Sensor tools
            "emit_reading": self._tool_emit_reading,
            "report_calibration": self._tool_report_calibration,
            # Gate scanner tools
            "scan_container": self._tool_scan_container,
            # Gate worker tools
            "verify_documents": self._tool_verify_documents,
            "process_truck": self._tool_process_truck,
            "inspect_seal": self._tool_inspect_seal,
            # TOC tools
            "update_terminal_summary": self._tool_update_terminal_summary,
            "authorize_resource_transfer": self._tool_authorize_resource_transfer,
            "emit_terminal_alert": self._tool_emit_terminal_alert,
            "adjust_zone_priority": self._tool_adjust_zone_priority,
            # Yard Manager tools
            "update_yard_summary": self._tool_update_yard_summary,
            "assign_yard_block": self._tool_assign_yard_block,
            "route_tractor": self._tool_route_tractor,
            "report_congestion": self._tool_report_congestion,
            # Stacking Crane tools
            "stack_container": self._tool_stack_container,
            "retrieve_container": self._tool_retrieve_container,
            # Yard Optimizer tools
            "publish_optimization_plan": self._tool_publish_optimization_plan,
            "allocate_block": self._tool_allocate_block,
            "match_backhaul": self._tool_match_backhaul,
            "emit_yard_optimization_event": self._tool_emit_yard_optimization_event,
            # Gate Flow AI tools
            "publish_gate_flow_plan": self._tool_publish_gate_flow_plan,
            "throttle_appointments": self._tool_throttle_appointments,
            "schedule_pickup": self._tool_schedule_pickup,
            "schedule_rail_loading": self._tool_schedule_rail_loading,
            "emit_gate_flow_event": self._tool_emit_gate_flow_event,
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

        # Complete the job
        self.store.complete_transport_job(self.hold_id, container_id)

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
        entity = self._get_entity_doc()
        if not entity:
            return "Error: entity document not found"

        # Stacking crane variant: crane_id, status, position, yard_block
        if "crane_id" in arguments or self.role == "stacking_crane":
            crane_id = arguments.get("crane_id", self.node_id)
            status = arguments.get("status", "idle")
            position = arguments.get("position", {})
            yard_block = arguments.get("yard_block", "?")
            fault = arguments.get("fault")
            container_id = arguments.get("container_id")

            if position:
                entity.update_field("position", position)
            entity.update_field("operational_status", "FAULT" if fault else status.upper())

            reports = entity.get_field("metrics.position_reports", 0)
            entity.update_field("metrics.position_reports", reports + 1)

            if fault:
                self.store.emit_event({
                    "event_type": "crane_fault",
                    "source": self.node_id,
                    "fault": fault,
                    "yard_block": yard_block,
                    "aggregation_policy": "IMMEDIATE_PROPAGATE",
                    "priority": "CRITICAL",
                })

            logger.info(
                f"METRICS: position_update node={self.node_id} "
                f"yard_block={yard_block} status={status}"
                + (f" fault={fault}" if fault else "")
            )
            return (
                f"Position reported: {yard_block}, status={status}."
                + (f" FAULT: {fault}" if fault else "")
            )

        # Tractor variant: zone, block, status
        zone = arguments["zone"]
        block = arguments["block"]
        status = arguments["status"]

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

    # ── Gate Scanner tool handlers ───────────────────────────────────────

    async def _tool_scan_container(self, arguments: dict) -> str:
        container_id = arguments.get("container_id", "UNKNOWN")
        measured_weight = arguments.get("measured_weight_tons")
        damage = arguments.get("damage_detected", False)
        within_tolerance = arguments.get("weight_within_tolerance", True)

        entity = self._get_entity_doc()
        if entity:
            scans = entity.get_field("metrics.scans_completed", 0)
            entity.update_field("metrics.scans_completed", scans + 1)

        event_priority = "NORMAL"
        if damage or not within_tolerance:
            event_priority = "HIGH"

        self.store.emit_event({
            "event_type": "container_scanned",
            "source": self.node_id,
            "container_id": container_id,
            "measured_weight_tons": measured_weight,
            "damage_detected": damage,
            "weight_within_tolerance": within_tolerance,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": event_priority,
        })

        logger.info(
            f"METRICS: container_scanned node={self.node_id} "
            f"container={container_id} weight={measured_weight}t "
            f"damage={damage} tolerance={within_tolerance}"
        )

        flags = []
        if damage:
            flags.append("DAMAGE")
        if not within_tolerance:
            flags.append("OVERWEIGHT")
        flag_str = f" FLAGS: {', '.join(flags)}" if flags else ""
        return f"Container {container_id} scanned.{flag_str}"

    # ── Gate Worker tool handlers ────────────────────────────────────────

    async def _tool_verify_documents(self, arguments: dict) -> str:
        container_id = arguments.get("container_id", "UNKNOWN")
        valid = arguments.get("documents_valid", False)

        entity = self._get_entity_doc()
        if entity:
            checks = entity.get_field("metrics.doc_checks", 0)
            entity.update_field("metrics.doc_checks", checks + 1)

        self.store.emit_event({
            "event_type": "documents_verified",
            "source": self.node_id,
            "container_id": container_id,
            "valid": valid,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL" if valid else "HIGH",
        })

        logger.info(
            f"METRICS: documents_verified node={self.node_id} "
            f"container={container_id} valid={valid}"
        )
        return f"Documents for {container_id}: {'VALID' if valid else 'INVALID'}."

    async def _tool_process_truck(self, arguments: dict) -> str:
        container_id = arguments.get("container_id", "UNKNOWN")
        action = arguments.get("action", "release")
        gate_lane = arguments.get("gate_lane", "unknown")

        entity = self._get_entity_doc()
        if entity:
            processed = entity.get_field("metrics.trucks_processed", 0)
            entity.update_field("metrics.trucks_processed", processed + 1)

        self.store.emit_event({
            "event_type": "truck_processed",
            "source": self.node_id,
            "container_id": container_id,
            "action": action,
            "gate_lane": gate_lane,
            "reasons": arguments.get("reasons", []),
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL" if action == "release" else "HIGH",
        })

        spatial_event = {
            "event_type": "spatial_update",
            "source": self.node_id,
            "priority": "ROUTINE",
            "details": {
                "operation": f"truck_{action}",
                "container_id": container_id,
                "gate_lane": gate_lane,
            },
        }
        print(json.dumps(spatial_event), flush=True)

        logger.info(
            f"METRICS: truck_processed node={self.node_id} "
            f"container={container_id} action={action} lane={gate_lane}"
        )
        return f"Truck {action}d: container {container_id} at {gate_lane}."

    async def _tool_inspect_seal(self, arguments: dict) -> str:
        container_id = arguments.get("container_id", "UNKNOWN")
        seal_intact = arguments.get("seal_intact", True)
        seal_match = arguments.get("seal_number_match", True)

        entity = self._get_entity_doc()
        if entity:
            inspections = entity.get_field("metrics.seal_inspections", 0)
            entity.update_field("metrics.seal_inspections", inspections + 1)

        compromised = not seal_intact or not seal_match

        self.store.emit_event({
            "event_type": "seal_inspected",
            "source": self.node_id,
            "container_id": container_id,
            "seal_intact": seal_intact,
            "seal_number_match": seal_match,
            "aggregation_policy": "AGGREGATE_AT_PARENT" if not compromised else "IMMEDIATE_PROPAGATE",
            "priority": "NORMAL" if not compromised else "CRITICAL",
        })

        logger.info(
            f"METRICS: seal_inspected node={self.node_id} "
            f"container={container_id} intact={seal_intact} match={seal_match}"
        )
        return f"Seal inspection for {container_id}: {'INTACT' if not compromised else 'COMPROMISED'}."

    # ── TOC tool handlers ────────────────────────────────────────────────

    async def _tool_update_terminal_summary(self, arguments: dict) -> str:
        total_moves_per_hour = arguments["total_moves_per_hour"]
        berth_statuses = arguments["berth_statuses"]
        completion_eta_minutes = arguments["completion_eta_minutes"]
        summary = arguments["summary"]
        total_moves_remaining = arguments.get("total_moves_remaining", 0)

        entity = self._get_entity_doc()
        if entity:
            summaries = entity.get_field("metrics.summaries_produced", 0)
            entity.update_field("metrics.summaries_produced", summaries + 1)

        self.store.emit_event({
            "event_type": "terminal_summary_update",
            "source": self.node_id,
            "total_moves_per_hour": total_moves_per_hour,
            "berth_statuses": berth_statuses,
            "completion_eta_minutes": completion_eta_minutes,
            "total_moves_remaining": total_moves_remaining,
            "summary": summary,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        logger.info(
            f"METRICS: terminal_summary_update node={self.node_id} "
            f"rate={total_moves_per_hour} eta={completion_eta_minutes}min "
            f"remaining={total_moves_remaining}"
        )
        return (
            f"Terminal summary updated: {total_moves_per_hour} moves/hr, "
            f"ETA {completion_eta_minutes} min. {summary}"
        )

    async def _tool_authorize_resource_transfer(self, arguments: dict) -> str:
        resource_type = arguments["resource_type"]
        from_zone = arguments["from_zone"]
        to_zone = arguments["to_zone"]
        reason = arguments["reason"]

        entity = self._get_entity_doc()
        if entity:
            transfers = entity.get_field("metrics.resource_transfers", 0)
            entity.update_field("metrics.resource_transfers", transfers + 1)

        self.store.emit_event({
            "event_type": "resource_transfer_authorized",
            "source": self.node_id,
            "resource_type": resource_type,
            "from_zone": from_zone,
            "to_zone": to_zone,
            "reason": reason,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": "HIGH",
        })

        logger.info(
            f"METRICS: resource_transfer node={self.node_id} "
            f"type={resource_type} {from_zone}->{to_zone}"
        )
        return (
            f"Resource transfer authorized: {resource_type} from {from_zone} "
            f"to {to_zone}. Reason: {reason}"
        )

    async def _tool_emit_terminal_alert(self, arguments: dict) -> str:
        alert_type = arguments["alert_type"]
        details = arguments["details"]
        severity = arguments["severity"]
        affected_zones = arguments.get("affected_zones", [])

        entity = self._get_entity_doc()
        if entity:
            alerts = entity.get_field("metrics.alerts_emitted", 0)
            entity.update_field("metrics.alerts_emitted", alerts + 1)

        self.store.emit_event({
            "event_type": f"terminal_alert_{alert_type}",
            "source": self.node_id,
            "alert_type": alert_type,
            "details": details,
            "severity": severity,
            "affected_zones": affected_zones,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": severity,
        })

        logger.info(
            f"METRICS: terminal_alert node={self.node_id} "
            f"type={alert_type} severity={severity} zones={affected_zones}"
        )
        return (
            f"Terminal alert emitted: {alert_type} ({severity}). {details}"
        )

    async def _tool_adjust_zone_priority(self, arguments: dict) -> str:
        zone_id = arguments["zone_id"]
        priority_level = arguments["priority_level"]
        reason = arguments["reason"]

        entity = self._get_entity_doc()
        if entity:
            adjustments = entity.get_field("metrics.zone_priority_adjustments", 0)
            entity.update_field("metrics.zone_priority_adjustments", adjustments + 1)

        self.store.emit_event({
            "event_type": "zone_priority_adjusted",
            "source": self.node_id,
            "zone_id": zone_id,
            "priority_level": priority_level,
            "reason": reason,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": "HIGH",
        })

        logger.info(
            f"METRICS: zone_priority node={self.node_id} "
            f"zone={zone_id} priority={priority_level}"
        )
        return (
            f"Zone priority adjusted: {zone_id} set to {priority_level}. "
            f"Reason: {reason}"
        )

    # ── Yard Manager tool handlers ───────────────────────────────────────

    async def _tool_update_yard_summary(self, arguments: dict) -> str:
        zone = arguments.get("zone", "default")
        utilization = arguments.get("utilization", 0.0)
        summary = arguments.get("summary", "")

        entity = self._get_entity_doc()
        if entity:
            summaries = entity.get_field("metrics.summaries_produced", 0)
            entity.update_field("metrics.summaries_produced", summaries + 1)

        self.store.emit_event({
            "event_type": "yard_summary_update",
            "source": self.node_id,
            "zone": zone,
            "utilization": utilization,
            "summary": summary,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        logger.info(
            f"METRICS: yard_summary_update node={self.node_id} "
            f"zone={zone} utilization={utilization}"
        )
        return f"Yard summary updated: {zone}, utilization={utilization:.1%}. {summary}"

    async def _tool_assign_yard_block(self, arguments: dict) -> str:
        container_id = arguments.get("container_id")
        block_id = arguments.get("block_id")
        action = arguments.get("action")

        entity = self._get_entity_doc()

        if action == "rebalance_crane":
            from_block = arguments.get("from_block", "?")
            to_block = arguments.get("to_block", "?")
            if entity:
                rebalances = entity.get_field("metrics.crane_rebalances", 0)
                entity.update_field("metrics.crane_rebalances", rebalances + 1)

            self.store.emit_event({
                "event_type": "crane_rebalanced",
                "source": self.node_id,
                "from_block": from_block,
                "to_block": to_block,
                "aggregation_policy": "AGGREGATE_AT_PARENT",
                "priority": "NORMAL",
            })

            logger.info(
                f"METRICS: crane_rebalanced node={self.node_id} "
                f"{from_block}->{to_block}"
            )
            return f"Crane rebalanced: {from_block} -> {to_block}."

        if container_id and block_id:
            self.store.emit_event({
                "event_type": "yard_block_assigned",
                "source": self.node_id,
                "container_id": container_id,
                "block_id": block_id,
                "aggregation_policy": "AGGREGATE_AT_PARENT",
                "priority": "NORMAL",
            })

            logger.info(
                f"METRICS: yard_block_assigned node={self.node_id} "
                f"container={container_id} block={block_id}"
            )
            return f"Container {container_id} assigned to block {block_id}."

        return "Yard block assignment: no action taken."

    async def _tool_route_tractor(self, arguments: dict) -> str:
        tractor_id = arguments["tractor_id"]
        target_block = arguments["target_block"]
        container_id = arguments.get("container_id")

        entity = self._get_entity_doc()
        if entity:
            routed = entity.get_field("metrics.tractors_routed", 0)
            entity.update_field("metrics.tractors_routed", routed + 1)

        self.store.emit_event({
            "event_type": "tractor_routed",
            "source": self.node_id,
            "tractor_id": tractor_id,
            "target_block": target_block,
            "container_id": container_id,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        logger.info(
            f"METRICS: tractor_routed node={self.node_id} "
            f"tractor={tractor_id} block={target_block}"
        )
        return f"Tractor {tractor_id} routed to {target_block}."

    async def _tool_report_congestion(self, arguments: dict) -> str:
        zone = arguments.get("zone", "unknown")
        block_id = arguments.get("block_id")
        queue_depth = arguments.get("queue_depth")
        escalate_to = arguments.get("escalate_to")
        reasons = arguments.get("reasons", [])

        entity = self._get_entity_doc()
        if entity:
            events = entity.get_field("metrics.congestion_events", 0)
            entity.update_field("metrics.congestion_events", events + 1)

        priority = "CRITICAL" if escalate_to else "HIGH"

        self.store.emit_event({
            "event_type": "congestion_reported",
            "source": self.node_id,
            "zone": zone,
            "block_id": block_id,
            "queue_depth": queue_depth,
            "escalate_to": escalate_to,
            "reasons": reasons,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": priority,
        })

        logger.info(
            f"METRICS: congestion_reported node={self.node_id} "
            f"zone={zone} block={block_id} escalate={escalate_to}"
        )
        if escalate_to:
            return f"Congestion escalated to {escalate_to}: zone={zone}. Reasons: {', '.join(reasons)}"
        return f"Congestion reported: block {block_id}, queue depth {queue_depth}."

    # ── Stacking Crane tool handlers ─────────────────────────────────────

    async def _tool_stack_container(self, arguments: dict) -> str:
        container_id = arguments.get("container_id", "UNKNOWN")
        row = arguments.get("row", 0)
        bay = arguments.get("bay", 0)
        tier = arguments.get("tier", 0)
        yard_block = arguments.get("yard_block", "?")
        crane_id = arguments.get("crane_id", self.node_id)

        entity = self._get_entity_doc()
        if entity:
            stacked = entity.get_field("metrics.containers_stacked", 0)
            entity.update_field("metrics.containers_stacked", stacked + 1)
            entity.update_field("position", {"row": row, "bay": bay})
            entity.update_field("hoist_load_kg", 0.0)
            entity.update_field("current_task", None)

        slot_key = f"{yard_block}:{row}:{bay}:{tier}"

        self.store.emit_event({
            "event_type": "container_stacked",
            "source": self.node_id,
            "container_id": container_id,
            "slot": slot_key,
            "yard_block": yard_block,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        spatial_event = {
            "event_type": "spatial_update",
            "source": self.node_id,
            "priority": "ROUTINE",
            "details": {
                "operation": "crane_stack",
                "crane_id": crane_id,
                "container_id": container_id,
                "yard_block": yard_block,
                "slot": slot_key,
            },
        }
        print(json.dumps(spatial_event), flush=True)

        logger.info(
            f"METRICS: container_stacked node={self.node_id} "
            f"container={container_id} slot={slot_key}"
        )
        return f"Container {container_id} stacked at {slot_key}."

    async def _tool_retrieve_container(self, arguments: dict) -> str:
        container_id = arguments.get("container_id", "UNKNOWN")
        row = arguments.get("row", 0)
        bay = arguments.get("bay", 0)
        tier = arguments.get("tier", 0)
        yard_block = arguments.get("yard_block", "?")
        crane_id = arguments.get("crane_id", self.node_id)

        entity = self._get_entity_doc()
        if entity:
            retrieved = entity.get_field("metrics.containers_retrieved", 0)
            entity.update_field("metrics.containers_retrieved", retrieved + 1)
            entity.update_field("position", {"row": row, "bay": bay})
            entity.update_field("current_task", None)

        slot_key = f"{yard_block}:{row}:{bay}:{tier}"

        self.store.emit_event({
            "event_type": "container_retrieved",
            "source": self.node_id,
            "container_id": container_id,
            "slot": slot_key,
            "yard_block": yard_block,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        spatial_event = {
            "event_type": "spatial_update",
            "source": self.node_id,
            "priority": "ROUTINE",
            "details": {
                "operation": "crane_retrieve",
                "crane_id": crane_id,
                "container_id": container_id,
                "yard_block": yard_block,
                "slot": slot_key,
            },
        }
        print(json.dumps(spatial_event), flush=True)

        logger.info(
            f"METRICS: container_retrieved node={self.node_id} "
            f"container={container_id} slot={slot_key}"
        )
        return f"Container {container_id} retrieved from {slot_key}."

    # ── Yard Optimizer tool handlers ─────────────────────────────────────

    async def _tool_publish_optimization_plan(self, arguments: dict) -> str:
        entity = self._get_entity_doc()
        if entity:
            entity.update_field("metrics.allocations_made",
                                entity.get_field("metrics.allocations_made", 0) + 1)
        self.store.emit_event({
            "event_type": "yard_optimization_plan",
            "source": self.node_id,
            "zone": arguments.get("zone"),
            "utilization": arguments.get("utilization"),
            "strategy": arguments.get("strategy"),
            "summary": arguments.get("summary"),
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })
        logger.info(
            f"METRICS: yard_optimization_plan node={self.node_id} "
            f"strategy={arguments.get('strategy')} "
            f"utilization={arguments.get('utilization')}"
        )
        return f"Yard optimization plan published: {arguments.get('summary')}"

    async def _tool_allocate_block(self, arguments: dict) -> str:
        container_id = arguments["container_id"]
        target_block = arguments["target_block"]
        entity = self._get_entity_doc()
        if entity:
            entity.update_field("metrics.allocations_made",
                                entity.get_field("metrics.allocations_made", 0) + 1)
        self.store.emit_event({
            "event_type": "block_allocation",
            "source": self.node_id,
            "container_id": container_id,
            "target_block": target_block,
            "container_type": arguments.get("container_type", "standard"),
            "reason": arguments.get("reason"),
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })
        logger.info(
            f"METRICS: block_allocation node={self.node_id} "
            f"container={container_id} block={target_block}"
        )
        return f"Container {container_id} allocated to block {target_block}."

    async def _tool_match_backhaul(self, arguments: dict) -> str:
        tractor_id = arguments["tractor_id"]
        pickup_block = arguments["pickup_block"]
        entity = self._get_entity_doc()
        if entity:
            entity.update_field("metrics.backhaul_matches",
                                entity.get_field("metrics.backhaul_matches", 0) + 1)
        self.store.emit_event({
            "event_type": "backhaul_match",
            "source": self.node_id,
            "tractor_id": tractor_id,
            "current_block": arguments.get("current_block"),
            "pickup_block": pickup_block,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })
        logger.info(
            f"METRICS: backhaul_match node={self.node_id} "
            f"tractor={tractor_id} pickup={pickup_block}"
        )
        return f"Tractor {tractor_id} matched with backhaul at {pickup_block}."

    async def _tool_emit_yard_optimization_event(self, arguments: dict) -> str:
        entity = self._get_entity_doc()
        if entity and arguments.get("event_type") == "congestion_redirect":
            entity.update_field("metrics.congestion_mitigations",
                                entity.get_field("metrics.congestion_mitigations", 0) + 1)
        self.store.emit_event({
            "event_type": arguments["event_type"],
            "source": self.node_id,
            "details": arguments.get("details"),
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": arguments.get("priority", "NORMAL"),
        })
        logger.info(
            f"METRICS: yard_optimization_event node={self.node_id} "
            f"type={arguments['event_type']} priority={arguments.get('priority')}"
        )
        return f"Yard optimization event emitted: {arguments['event_type']}"

    # ── Gate Flow AI tool handlers ───────────────────────────────────────

    async def _tool_publish_gate_flow_plan(self, arguments: dict) -> str:
        entity = self._get_entity_doc()
        self.store.emit_event({
            "event_type": "gate_flow_plan",
            "source": self.node_id,
            "queue_length": arguments.get("queue_length"),
            "throttle_active": arguments.get("throttle_active"),
            "summary": arguments.get("summary"),
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })
        logger.info(
            f"METRICS: gate_flow_plan node={self.node_id} "
            f"queue={arguments.get('queue_length')} "
            f"throttle={arguments.get('throttle_active')}"
        )
        return f"Gate flow plan published: {arguments.get('summary')}"

    async def _tool_throttle_appointments(self, arguments: dict) -> str:
        entity = self._get_entity_doc()
        if entity:
            entity.update_field("metrics.throttle_activations",
                                entity.get_field("metrics.throttle_activations", 0) + 1)
        self.store.emit_event({
            "event_type": "appointment_throttle",
            "source": self.node_id,
            "action": arguments["action"],
            "current_queue": arguments.get("current_queue"),
            "target_queue": arguments.get("target_queue"),
            "reason": arguments.get("reason"),
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": "HIGH",
        })
        logger.info(
            f"METRICS: appointment_throttle node={self.node_id} "
            f"action={arguments['action']}"
        )
        return f"Appointment throttle: {arguments['action']} — {arguments.get('reason')}"

    async def _tool_schedule_pickup(self, arguments: dict) -> str:
        truck_id = arguments["truck_id"]
        container_id = arguments["container_id"]
        entity = self._get_entity_doc()
        if entity:
            entity.update_field("metrics.appointments_scheduled",
                                entity.get_field("metrics.appointments_scheduled", 0) + 1)
        self.store.emit_event({
            "event_type": "pickup_scheduled",
            "source": self.node_id,
            "truck_id": truck_id,
            "container_id": container_id,
            "gate_lane": arguments.get("gate_lane"),
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": arguments.get("priority", "NORMAL"),
        })
        logger.info(
            f"METRICS: pickup_scheduled node={self.node_id} "
            f"truck={truck_id} container={container_id}"
        )
        return f"Pickup scheduled: truck {truck_id} → container {container_id}."

    async def _tool_schedule_rail_loading(self, arguments: dict) -> str:
        rail_track = arguments["rail_track"]
        containers = arguments.get("containers", [])
        entity = self._get_entity_doc()
        if entity:
            entity.update_field("metrics.rail_loads_coordinated",
                                entity.get_field("metrics.rail_loads_coordinated", 0) + 1)
        self.store.emit_event({
            "event_type": "rail_loading_scheduled",
            "source": self.node_id,
            "rail_track": rail_track,
            "container_count": len(containers),
            "containers": containers,
            "window_start": arguments.get("window_start"),
            "estimated_duration_min": arguments.get("estimated_duration_min"),
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })
        logger.info(
            f"METRICS: rail_loading_scheduled node={self.node_id} "
            f"track={rail_track} containers={len(containers)}"
        )
        return f"Rail loading scheduled: {len(containers)} containers on {rail_track}."

    async def _tool_emit_gate_flow_event(self, arguments: dict) -> str:
        self.store.emit_event({
            "event_type": arguments["event_type"],
            "source": self.node_id,
            "details": arguments.get("details"),
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": arguments.get("priority", "NORMAL"),
        })
        logger.info(
            f"METRICS: gate_flow_event node={self.node_id} "
            f"type={arguments['event_type']} priority={arguments.get('priority')}"
        )
        return f"Gate flow event emitted: {arguments['event_type']}"
