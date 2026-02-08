"""
HIVE State Store — manages entity state documents.

Phase 0: In-memory store with HIVE-compatible document structure.
Phase 1+: Swap for real HIVE CRDT backend via hive-transport REST API.
"""

from __future__ import annotations

import asyncio
import json
import time
from dataclasses import dataclass, field
from typing import Any, Optional


@dataclass
class HiveDocument:
    """A HIVE-compatible document (mirrors CRDT document structure)."""
    doc_id: str
    collection: str
    fields: dict[str, Any]
    created_at_us: int = field(default_factory=lambda: int(time.time() * 1_000_000))
    last_modified_us: int = field(default_factory=lambda: int(time.time() * 1_000_000))

    def update_field(self, path: str, value: Any) -> None:
        """Update a field by dot-notation path."""
        parts = path.split(".")
        target = self.fields
        for part in parts[:-1]:
            target = target.setdefault(part, {})
        target[parts[-1]] = value
        self.last_modified_us = int(time.time() * 1_000_000)

    def get_field(self, path: str, default: Any = None) -> Any:
        """Get a field by dot-notation path."""
        parts = path.split(".")
        target = self.fields
        for part in parts:
            if isinstance(target, dict) and part in target:
                target = target[part]
            else:
                return default
        return target

    def to_json(self) -> dict:
        return {
            "doc_id": self.doc_id,
            "collection": self.collection,
            "fields": self.fields,
            "created_at_us": self.created_at_us,
            "last_modified_us": self.last_modified_us,
        }


class HiveStateStore:
    """
    In-memory HIVE state store for Phase 0.

    Maintains documents organized by collection, matching the structure
    that hive-sim-node would produce via CRDT sync.
    """

    def __init__(self):
        self._collections: dict[str, dict[str, HiveDocument]] = {}
        self._event_log: list[dict] = []
        self._queue_lock = asyncio.Lock()

    def create_document(self, collection: str, doc_id: str, fields: dict) -> HiveDocument:
        """Create a new document (create-once pattern per ADR-021)."""
        if collection not in self._collections:
            self._collections[collection] = {}

        doc = HiveDocument(doc_id=doc_id, collection=collection, fields=fields)
        self._collections[collection][doc_id] = doc
        return doc

    def get_document(self, collection: str, doc_id: str) -> HiveDocument | None:
        """Get a document by collection and ID."""
        return self._collections.get(collection, {}).get(doc_id)

    def query_collection(self, collection: str) -> list[HiveDocument]:
        """Query all documents in a collection."""
        return list(self._collections.get(collection, {}).values())

    def update_document(self, collection: str, doc_id: str, path: str, value: Any) -> bool:
        """Update a field in an existing document (delta update pattern)."""
        doc = self.get_document(collection, doc_id)
        if doc is None:
            return False
        doc.update_field(path, value)
        return True

    def emit_event(self, event: dict) -> None:
        """Emit a HIVE event (per ADR-027 event routing)."""
        event["timestamp_us"] = int(time.time() * 1_000_000)
        self._event_log.append(event)

    def get_events(self, since_us: int = 0, event_type: str | None = None) -> list[dict]:
        """Query events since a timestamp, optionally filtered by type."""
        events = [e for e in self._event_log if e["timestamp_us"] > since_us]
        if event_type:
            events = [e for e in events if e.get("event_type") == event_type]
        return events

    async def claim_next_container(
        self, hold_id: str, container_id: str, claimer_node_id: str
    ) -> bool:
        """
        Atomically claim a container from the queue.

        Returns True if the claim succeeded, False if already claimed or not found.
        Uses asyncio.Lock to prevent double-booking when multiple cranes compete.
        """
        async with self._queue_lock:
            queue_doc = self.get_document("container_queues", f"queue_{hold_id}")
            if queue_doc is None:
                return False

            containers = queue_doc.fields.get("containers", [])
            for c in containers:
                if c["container_id"] == container_id:
                    # Already claimed or completed
                    if c.get("claimed_by") or c.get("status") == "COMPLETED":
                        return False
                    # Claim it
                    c["claimed_by"] = claimer_node_id
                    c["status"] = "COMPLETED"
                    # Advance queue counters
                    completed = queue_doc.fields.get("completed_count", 0)
                    queue_doc.update_field("completed_count", completed + 1)
                    next_idx = queue_doc.fields.get("next_index", 0)
                    # Advance next_index past all claimed/completed containers
                    while next_idx < len(containers) and containers[next_idx].get("claimed_by"):
                        next_idx += 1
                    queue_doc.update_field("next_index", next_idx)
                    return True

            return False

    def request_operator(self, crane_id: str, hold_id: str, requirements: dict | None = None) -> str:
        """Post an operator request from a crane. Returns request doc_id."""
        req_id = f"req_{hold_id}_{crane_id}"
        self.create_document(
            collection="operator_requests",
            doc_id=req_id,
            fields={
                "crane_id": crane_id,
                "hold_id": hold_id,
                "requirements": requirements or {},
                "status": "PENDING",
                "assigned_operator": None,
            },
        )
        return req_id

    async def assign_operator(self, operator_id: str, crane_id: str) -> bool:
        """Atomically assign an operator to a crane."""
        async with self._queue_lock:
            op_doc = self.get_document("node_states", f"sim_doc_{operator_id}")
            if not op_doc:
                return False
            current = op_doc.get_field("assignment.assigned_to")
            if current is not None:
                return False
            op_doc.update_field("assignment.assigned_to", crane_id)
            op_doc.update_field("operational_status", "BUSY")
            return True

    async def release_operator(self, operator_id: str) -> bool:
        """Release an operator back to the available pool."""
        async with self._queue_lock:
            op_doc = self.get_document("node_states", f"sim_doc_{operator_id}")
            if not op_doc:
                return False
            op_doc.update_field("assignment.assigned_to", None)
            op_doc.update_field("operational_status", "AVAILABLE")
            # Increment assist count
            assists = op_doc.get_field("metrics.moves_assisted", 0)
            op_doc.update_field("metrics.moves_assisted", assists + 1)
            return True

    def get_pending_operator_requests(self, hold_id: str) -> list[HiveDocument]:
        """Get all pending operator requests for a hold."""
        docs = self.query_collection("operator_requests")
        return [
            d for d in docs
            if d.fields.get("hold_id") == hold_id
            and d.fields.get("status") == "PENDING"
        ]


def create_crane_entity(store: HiveStateStore, node_id: str, config: dict) -> HiveDocument:
    """Initialize a gantry crane entity document in the HIVE state store."""
    return store.create_document(
        collection="node_states",
        doc_id=f"sim_doc_{node_id}",
        fields={
            "node_id": node_id,
            "entity_type": "gantry_crane",
            "hive_level": "H1",
            "operational_status": "OPERATIONAL",
            "capabilities": {
                "container_lift": {
                    "type": "CONTAINER_LIFT",
                    "lift_capacity_tons": config.get("lift_capacity_tons", 65),
                    "reach_rows": config.get("reach_rows", 22),
                    "moves_per_hour": config.get("moves_per_hour", 30),
                    "current_rate": 0,
                    "status": "READY",
                },
                "hazmat_rated": {
                    "type": "HAZMAT_RATED",
                    "classes": config.get("hazmat_classes", [1, 3, 8, 9]),
                    "certification_valid": config.get("hazmat_cert_valid", True),
                },
            },
            "equipment_health": {
                "hydraulic_pct": 100,
                "spreader_alignment": "NORMAL",
                "electrical_status": "NORMAL",
            },
            "assignment": {
                "berth": config.get("berth", "berth-5"),
                "hold": config.get("hold", 3),
                "vessel": config.get("vessel", "MV Ever Forward"),
            },
            "metrics": {
                "moves_completed": 0,
                "moves_failed": 0,
                "total_tons_lifted": 0.0,
                "session_start_us": int(time.time() * 1_000_000),
            },
        },
    )


def create_container_queue(store: HiveStateStore, hold_id: str, containers: list[dict]) -> HiveDocument:
    """Initialize the container queue document for a hold."""
    return store.create_document(
        collection="container_queues",
        doc_id=f"queue_{hold_id}",
        fields={
            "hold_id": hold_id,
            "containers": containers,
            "next_index": 0,
            "total_containers": len(containers),
            "completed_count": 0,
        },
    )


def create_team_state(store: HiveStateStore, hold_id: str) -> HiveDocument:
    """Initialize the hold team state summary document."""
    return store.create_document(
        collection="team_summaries",
        doc_id=f"team_{hold_id}",
        fields={
            "hold_id": hold_id,
            "team_members": {},
            "aggregated_capabilities": {},
            "moves_per_hour": 0,
            "target_moves_per_hour": 35,
            "moves_completed": 0,
            "moves_remaining": 0,
            "gap_analysis": [],
            "status": "FORMING",
        },
    )


def create_operator_entity(store: HiveStateStore, node_id: str, config: dict) -> HiveDocument:
    """Initialize a crane operator entity document in the HIVE state store."""
    return store.create_document(
        collection="node_states",
        doc_id=f"sim_doc_{node_id}",
        fields={
            "node_id": node_id,
            "entity_type": "operator",
            "hive_level": "H1",
            "operational_status": "AVAILABLE",
            "capabilities": {
                "crane_operation": {
                    "type": "CRANE_OPERATION",
                    "proficiency": config.get("proficiency", "expert"),
                    "certification_id": "OSHA_1926.1400",
                    "certification_valid": config.get("osha_cert_valid", True),
                },
                "hazmat_handling": {
                    "type": "HAZMAT_HANDLING",
                    "classes": config.get("hazmat_classes", [3, 8, 9]),
                    "certification_valid": config.get("hazmat_cert_valid", True),
                },
            },
            "shift": {
                "start": config.get("shift_start", "06:00"),
                "end": config.get("shift_end", "18:00"),
                "status": "ON_SHIFT",
            },
            "assignment": {
                "assigned_to": None,
                "berth": config.get("berth", "berth-5"),
                "hold": config.get("hold", 3),
            },
            "metrics": {
                "moves_assisted": 0,
                "hazmat_inspections": 0,
                "session_start_us": int(time.time() * 1_000_000),
            },
        },
    )


def create_sample_containers(count: int = 20, hazmat_count: int = 3) -> list[dict]:
    """Generate sample container queue for Phase 0 testing."""
    containers = []
    for i in range(count):
        is_hazmat = i < hazmat_count
        container = {
            "container_id": f"MSCU-{4472891 + i}",
            "weight_tons": 25.0 + (i % 10) * 2.5,
            "size_teu": 2 if i % 4 == 0 else 1,
            "hazmat": is_hazmat,
            "hazmat_class": 3 if is_hazmat else None,
            "destination_block": f"YB-{chr(65 + (i % 6))}{(i % 20) + 1:02d}",
            "status": "QUEUED",
        }
        containers.append(container)
    return containers
