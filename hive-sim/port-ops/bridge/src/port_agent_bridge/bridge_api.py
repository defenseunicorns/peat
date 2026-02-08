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
    ):
        self.store = store
        self.node_id = node_id
        self.role = role
        self.hold_id = hold_id
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

        elif uri == "hive://tasking":
            if self.role == "aggregator":
                text = self._read_aggregator_tasking()
            elif self.role == "operator":
                text = self._read_operator_tasking()
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

    async def list_tools(self) -> ListToolsResultShim:
        """Return tools appropriate for this agent's role."""
        if self.role == "aggregator":
            return ListToolsResultShim(tools=list(AGGREGATOR_TOOLS))
        elif self.role == "operator":
            return ListToolsResultShim(tools=list(OPERATOR_TOOLS))
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

        logger.info(
            f"METRICS: container_move_complete node={self.node_id} "
            f"container={container_id} weight={weight}t "
            f"total_moves={moves + 1}"
        )

        remaining_count = queue.fields["total_containers"] - queue.fields.get("completed_count", 0)
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
