"""
Port Agent MCP Bridge Server

Exposes HIVE state as MCP resources and tools for Gastown agents.
This is the interface layer between AI agents and the HIVE protocol.

Resources (read):
  - hive://my-capabilities     — this node's capability document
  - hive://team-state          — aggregated team summary
  - hive://container-queue     — assigned container sequence
  - hive://tasking             — current directives

Tools (write):
  - update_capability          — update capability assertions
  - complete_container_move    — report completed container move
  - report_equipment_status    — report status change
  - request_support            — escalate capability gap
"""

from __future__ import annotations

import json
import logging
import os
import time

from mcp.server import Server
from mcp.server.lowlevel.server import InitializationOptions
from mcp.server.stdio import stdio_server
from mcp.types import (
    Resource,
    ServerCapabilities,
    Tool,
    TextContent,
)

from .hive_state import (
    HiveStateStore,
    create_crane_entity,
    create_container_queue,
    create_team_state,
    create_sample_containers,
)

logger = logging.getLogger(__name__)

# Global state store (Phase 0: in-memory, Phase 1+: HIVE CRDT backend)
_store = HiveStateStore()
_node_id: str = ""
_entity_doc_id: str = ""


def _get_entity_doc():
    """Get this node's entity document."""
    return _store.get_document("node_states", _entity_doc_id)


def _get_team_doc():
    """Get the team summary document."""
    hold_id = f"hold-{_get_entity_doc().get_field('assignment.hold')}"
    return _store.get_document("team_summaries", f"team_{hold_id}")


def _get_queue_doc():
    """Get the container queue document."""
    hold_id = f"hold-{_get_entity_doc().get_field('assignment.hold')}"
    return _store.get_document("container_queues", f"queue_{hold_id}")


def create_server(node_id: str, entity_config: dict) -> Server:
    """Create and configure the MCP bridge server."""
    global _node_id, _entity_doc_id, _store

    _node_id = node_id
    _entity_doc_id = f"sim_doc_{node_id}"

    # Initialize HIVE state for Phase 0
    _store = HiveStateStore()
    create_crane_entity(_store, node_id, entity_config)

    hold_num = entity_config.get("hold", 3)
    hold_id = f"hold-{hold_num}"
    containers = create_sample_containers(
        count=entity_config.get("queue_size", 20),
        hazmat_count=entity_config.get("hazmat_count", 3),
    )
    create_container_queue(_store, hold_id, containers)
    create_team_state(_store, hold_id)

    # Populate team state with this crane as a member
    team_doc = _store.get_document("team_summaries", f"team_{hold_id}")
    if team_doc:
        team_doc.update_field(f"team_members.{node_id}", {
            "entity_type": "gantry_crane",
            "status": "OPERATIONAL",
            "capabilities": ["CONTAINER_LIFT", "HAZMAT_RATED"],
        })
        team_doc.update_field("moves_remaining", len(containers))
        team_doc.update_field("status", "ACTIVE")

    # Add a simulated worker (operator ready)
    worker_id = "worker-lead"
    _store.create_document(
        collection="node_states",
        doc_id=f"sim_doc_{worker_id}",
        fields={
            "node_id": worker_id,
            "entity_type": "worker",
            "hive_level": "H1",
            "operational_status": "OPERATIONAL",
            "capabilities": {
                "crane_operation": {
                    "type": "CRANE_OPERATION",
                    "proficiency": "expert",
                    "certification_id": "OSHA_1926.1400",
                    "certification_valid": True,
                },
                "hazmat_handling": {
                    "type": "HAZMAT_HANDLING",
                    "proficiency": "competent",
                    "classes": [3, 8, 9],
                    "certification_valid": False,
                    "certification_expiry": "2025-12-01",
                    "evidence_handling_count": 47,
                    "evidence_incident_count": 0,
                },
            },
            "assignment": {
                "berth": "berth-5",
                "hold": hold_num,
                "crane": node_id,
            },
            "shift": {
                "start": "0600",
                "end": "1800",
                "status": "ON_SHIFT",
            },
        },
    )

    if team_doc:
        team_doc.update_field(f"team_members.{worker_id}", {
            "entity_type": "worker",
            "status": "OPERATIONAL",
            "capabilities": ["CRANE_OPERATION", "HAZMAT_HANDLING"],
            "hazmat_cert_valid": False,
        })

    server = Server("port-agent-bridge")

    # =========================================================================
    # Resources — agent reads HIVE state
    # =========================================================================

    @server.list_resources()
    async def list_resources():
        return [
            Resource(
                uri="hive://my-capabilities",
                name="My Capabilities",
                description="This node's capability document from HIVE state",
                mimeType="application/json",
            ),
            Resource(
                uri="hive://team-state",
                name="Team State",
                description="Aggregated hold team summary",
                mimeType="application/json",
            ),
            Resource(
                uri="hive://container-queue",
                name="Container Queue",
                description="Assigned container sequence for this hold",
                mimeType="application/json",
            ),
            Resource(
                uri="hive://tasking",
                name="Current Tasking",
                description="Active directives and assignments",
                mimeType="application/json",
            ),
            Resource(
                uri="hive://debug/state-dump",
                name="Full State Dump",
                description="All HIVE documents and events (debug/dashboard use)",
                mimeType="application/json",
            ),
        ]

    @server.read_resource()
    async def read_resource(uri):
        uri = str(uri)  # MCP SDK 1.26+ passes AnyUrl, not str
        if uri == "hive://my-capabilities":
            doc = _get_entity_doc()
            if doc:
                return json.dumps(doc.fields, indent=2, default=str)
            return json.dumps({"error": "Entity document not found"})

        elif uri == "hive://team-state":
            doc = _get_team_doc()
            if doc:
                return json.dumps(doc.fields, indent=2, default=str)
            return json.dumps({"error": "Team state not found"})

        elif uri == "hive://container-queue":
            doc = _get_queue_doc()
            if doc:
                # Return next few containers, not the entire queue
                fields = doc.fields.copy()
                next_idx = fields.get("next_index", 0)
                all_containers = fields.get("containers", [])
                fields["next_containers"] = all_containers[next_idx:next_idx + 5]
                fields["containers"] = f"[{len(all_containers)} total, showing next 5]"
                return json.dumps(fields, indent=2, default=str)
            return json.dumps({"error": "Container queue not found"})

        elif uri == "hive://tasking":
            # Phase 0: tasking is derived from assignment + queue state
            entity = _get_entity_doc()
            queue = _get_queue_doc()
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

        elif uri == "hive://debug/state-dump":
            # Full state dump: all documents across all collections + event log
            dump = {"documents": {}, "events": _store.get_events()}
            for col_name, col_docs in _store._collections.items():
                dump["documents"][col_name] = {}
                for doc_id, doc in col_docs.items():
                    dump["documents"][col_name][doc_id] = doc.fields
            return json.dumps(dump, indent=2, default=str)

        return json.dumps({"error": f"Unknown resource: {uri}"})

    # =========================================================================
    # Tools — agent writes HIVE state
    # =========================================================================

    @server.list_tools()
    async def list_tools():
        return [
            Tool(
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
            Tool(
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
            Tool(
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
            Tool(
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

    @server.call_tool()
    async def call_tool(name: str, arguments: dict):
        if name == "update_capability":
            field_path = arguments["field"]
            value = arguments["value"]

            doc = _get_entity_doc()
            if doc:
                doc.update_field(field_path, value)
                _store.emit_event({
                    "event_type": "capability_update",
                    "source": _node_id,
                    "field": field_path,
                    "value": value,
                    "aggregation_policy": "AGGREGATE_AT_PARENT",
                    "priority": "NORMAL",
                })
                logger.info(f"METRICS: capability_update node={_node_id} field={field_path} value={value}")
                return [TextContent(
                    type="text",
                    text=f"Updated {field_path} to {value}. HIVE state synced.",
                )]
            return [TextContent(type="text", text="Error: entity document not found")]

        elif name == "complete_container_move":
            container_id = arguments["container_id"]
            queue = _get_queue_doc()
            entity = _get_entity_doc()

            if not queue or not entity:
                return [TextContent(type="text", text="Error: queue or entity not found")]

            # Advance queue
            next_idx = queue.fields.get("next_index", 0)
            containers = queue.fields.get("containers", [])
            completed = queue.fields.get("completed_count", 0)

            # Mark container as completed
            found = False
            for c in containers:
                if c["container_id"] == container_id:
                    c["status"] = "COMPLETED"
                    found = True
                    break

            if not found:
                return [TextContent(type="text", text=f"Error: container {container_id} not in queue")]

            queue.update_field("next_index", next_idx + 1)
            queue.update_field("completed_count", completed + 1)

            # Update crane metrics
            moves = entity.get_field("metrics.moves_completed", 0)
            entity.update_field("metrics.moves_completed", moves + 1)
            weight = next(
                (c["weight_tons"] for c in containers if c["container_id"] == container_id),
                25.0,
            )
            total_tons = entity.get_field("metrics.total_tons_lifted", 0.0)
            entity.update_field("metrics.total_tons_lifted", total_tons + weight)

            # Emit event
            _store.emit_event({
                "event_type": "container_move_complete",
                "source": _node_id,
                "container_id": container_id,
                "weight_tons": weight,
                "aggregation_policy": "AGGREGATE_AT_PARENT",
                "priority": "NORMAL",
            })

            # Update team summary
            team = _get_team_doc()
            if team:
                team_completed = team.get_field("moves_completed", 0)
                team.update_field("moves_completed", team_completed + 1)
                remaining = team.get_field("moves_remaining", 0)
                team.update_field("moves_remaining", max(0, remaining - 1))

            logger.info(
                f"METRICS: container_move_complete node={_node_id} "
                f"container={container_id} weight={weight}t "
                f"total_moves={moves + 1}"
            )

            remaining_count = queue.fields["total_containers"] - (completed + 1)
            return [TextContent(
                type="text",
                text=(
                    f"Container {container_id} move completed. "
                    f"Weight: {weight}t. Total moves: {moves + 1}. "
                    f"Remaining in queue: {remaining_count}."
                ),
            )]

        elif name == "report_equipment_status":
            status = arguments["status"]
            details = arguments["details"]

            entity = _get_entity_doc()
            if not entity:
                return [TextContent(type="text", text="Error: entity document not found")]

            entity.update_field("operational_status", status)

            # Determine event priority based on severity
            priority = "NORMAL"
            if status in ("DEGRADED", "FAILED"):
                priority = "CRITICAL"

            _store.emit_event({
                "event_type": "equipment_status_change",
                "source": _node_id,
                "status": status,
                "details": details,
                "aggregation_policy": "IMMEDIATE_PROPAGATE",
                "priority": priority,
            })

            logger.info(
                f"METRICS: equipment_status_change node={_node_id} "
                f"status={status} details={details}"
            )
            return [TextContent(
                type="text",
                text=f"Equipment status updated to {status}. Event propagated with {priority} priority.",
            )]

        elif name == "request_support":
            capability_needed = arguments["capability_needed"]
            reason = arguments["reason"]

            _store.emit_event({
                "event_type": "support_request",
                "source": _node_id,
                "capability_needed": capability_needed,
                "reason": reason,
                "aggregation_policy": "IMMEDIATE_PROPAGATE",
                "priority": "HIGH",
            })

            # Update team gap analysis
            team = _get_team_doc()
            if team:
                gaps = team.get_field("gap_analysis", [])
                gaps.append({
                    "capability": capability_needed,
                    "reason": reason,
                    "reported_by": _node_id,
                    "timestamp_us": int(time.time() * 1_000_000),
                })
                team.update_field("gap_analysis", gaps)

            logger.info(
                f"METRICS: support_request node={_node_id} "
                f"capability={capability_needed} reason={reason}"
            )
            return [TextContent(
                type="text",
                text=(
                    f"Support request submitted: need {capability_needed}. "
                    f"Reason: {reason}. Escalated to hold aggregator."
                ),
            )]

        return [TextContent(type="text", text=f"Unknown tool: {name}")]

    return server


async def run_server(node_id: str, entity_config: dict):
    """Run the MCP bridge server via stdio transport."""
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
    )
    server = create_server(node_id, entity_config)
    init_options = InitializationOptions(
        server_name="port-agent-bridge",
        server_version="0.1.0",
        capabilities=ServerCapabilities(
            resources={},
            tools={},
        ),
    )
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, init_options)
