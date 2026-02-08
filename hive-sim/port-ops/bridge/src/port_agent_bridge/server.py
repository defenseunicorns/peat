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

Phase 0 single-agent mode works via MCP stdio transport.
Phase 1a multi-agent mode uses BridgeAPI directly (no MCP transport).
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

from .bridge_api import BridgeAPI
from .hive_state import (
    HiveStateStore,
    create_crane_entity,
    create_container_queue,
    create_team_state,
    create_sample_containers,
)

logger = logging.getLogger(__name__)


def create_server(node_id: str, entity_config: dict) -> Server:
    """Create and configure the MCP bridge server."""

    # Initialize HIVE state for Phase 0
    store = HiveStateStore()
    create_crane_entity(store, node_id, entity_config)

    hold_num = entity_config.get("hold", 3)
    hold_id = f"hold-{hold_num}"
    containers = create_sample_containers(
        count=entity_config.get("queue_size", 20),
        hazmat_count=entity_config.get("hazmat_count", 3),
    )
    create_container_queue(store, hold_id, containers)
    create_team_state(store, hold_id)

    # Populate team state with this crane as a member
    team_doc = store.get_document("team_summaries", f"team_{hold_id}")
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
    store.create_document(
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

    # Create BridgeAPI instance for this server — delegates all logic
    api = BridgeAPI(store=store, node_id=node_id, role="crane", hold_id=hold_id)

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
        result = await api.read_resource(uri)
        return result.contents[0].text

    # =========================================================================
    # Tools — agent writes HIVE state
    # =========================================================================

    @server.list_tools()
    async def list_tools():
        shim_result = await api.list_tools()
        return [
            Tool(
                name=t.name,
                description=t.description,
                inputSchema=t.inputSchema,
            )
            for t in shim_result.tools
        ]

    @server.call_tool()
    async def call_tool(name: str, arguments: dict):
        result = await api.call_tool(name, arguments)
        return [TextContent(type="text", text=result.content[0].text)]

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
