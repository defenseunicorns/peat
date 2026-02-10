"""
Bridge API - Entity state management for port operations simulation.

Tracks all simulation entities (workers, cranes, holds, containers) and
applies scenario event mutations. Provides the state layer that the
orchestrator queries and the LLM decision engine reasons over.

Entity lifecycle: REGISTERING -> ACTIVE -> DEGRADED/RECERTIFYING -> OFFLINE
"""

import threading
import time
from collections import defaultdict
from dataclasses import dataclass, field, asdict
from enum import Enum
from typing import Optional


class EntityStatus(str, Enum):
    REGISTERING = "REGISTERING"
    ACTIVE = "ACTIVE"
    DEGRADED = "DEGRADED"
    RECERTIFYING = "RECERTIFYING"
    OFFLINE = "OFFLINE"


@dataclass
class Capability:
    name: str
    status: EntityStatus = EntityStatus.ACTIVE
    capacity_pct: int = 100
    cert_valid: bool = True
    cert_expiry: Optional[str] = None
    details: str = ""


@dataclass
class Worker:
    worker_id: str
    shift: str
    capabilities: dict[str, Capability] = field(default_factory=dict)
    status: EntityStatus = EntityStatus.REGISTERING
    assigned_hold: Optional[str] = None
    role: str = "stevedore"

    def has_capability(self, cap_name: str, active_only: bool = True) -> bool:
        cap = self.capabilities.get(cap_name)
        if cap is None:
            return False
        if active_only:
            return cap.status == EntityStatus.ACTIVE and cap.cert_valid
        return True


@dataclass
class Crane:
    crane_id: str
    hold_id: str
    crane_type: str
    moves_per_hour: int
    nominal_moves_per_hour: int
    status: EntityStatus = EntityStatus.ACTIVE
    fault_type: Optional[str] = None
    capacity_pct: int = 100


@dataclass
class Hold:
    hold_id: str
    containers_total: int
    containers_remaining: int
    hazmat_containers: int
    crane_id: str
    team_members: list[str] = field(default_factory=list)
    crane_operator: Optional[str] = None
    supervisor: Optional[str] = None
    priority: str = "normal"
    eta_hours: float = 0.0


@dataclass
class Berth:
    berth_id: str
    vessel_name: str
    holds: list[str] = field(default_factory=list)
    total_containers: int = 0
    containers_moved: int = 0
    start_cycle: int = 0


class EntityStateManager:
    """Thread-safe entity state manager for port operations simulation."""

    def __init__(self):
        self.lock = threading.Lock()
        self.workers: dict[str, Worker] = {}
        self.cranes: dict[str, Crane] = {}
        self.holds: dict[str, Hold] = {}
        self.berth: Optional[Berth] = None
        self.event_log: list[dict] = []
        self._cycle: int = 0

    @property
    def cycle(self) -> int:
        return self._cycle

    @cycle.setter
    def cycle(self, value: int):
        self._cycle = value

    def load_scenario_entities(self, scenario: dict) -> None:
        """Initialize entities from scenario definition."""
        with self.lock:
            # Load cranes
            for crane_def in scenario.get("entities", {}).get("cranes", []):
                self.cranes[crane_def["id"]] = Crane(
                    crane_id=crane_def["id"],
                    hold_id=crane_def["hold"],
                    crane_type=crane_def["type"],
                    moves_per_hour=crane_def["moves_per_hour"],
                    nominal_moves_per_hour=crane_def["moves_per_hour"],
                )

            # Load workers
            for worker_def in scenario.get("entities", {}).get("workers", []):
                caps = {}
                for cap_name in worker_def.get("capabilities", []):
                    cert_valid = True
                    if cap_name == "hazmat":
                        cert_valid = worker_def.get("hazmat_cert_valid", False)
                    caps[cap_name] = Capability(
                        name=cap_name,
                        cert_valid=cert_valid,
                    )
                self.workers[worker_def["id"]] = Worker(
                    worker_id=worker_def["id"],
                    shift=worker_def["shift"],
                    capabilities=caps,
                    status=EntityStatus.REGISTERING,
                )

            # Load holds
            for hold_def in scenario.get("entities", {}).get("holds", []):
                self.holds[hold_def["id"]] = Hold(
                    hold_id=hold_def["id"],
                    containers_total=hold_def["containers"],
                    containers_remaining=hold_def["containers"],
                    hazmat_containers=hold_def["hazmat_containers"],
                    crane_id=hold_def["crane"],
                )

            # Load berth
            vessel = scenario.get("vessel", {})
            if vessel:
                self.berth = Berth(
                    berth_id=vessel.get("berth", "berth-1"),
                    vessel_name=vessel.get("name", "Unknown"),
                    holds=[h["id"] for h in scenario.get("entities", {}).get("holds", [])],
                    total_containers=vessel.get("containers_total", 0),
                )

    def _log_event(self, event_type: str, details: dict) -> None:
        self.event_log.append({
            "cycle": self._cycle,
            "timestamp": time.time(),
            "type": event_type,
            **details,
        })

    # --- Event handlers ---

    def handle_vessel_manifest(self, payload: dict) -> dict:
        """Process vessel arrival manifest."""
        with self.lock:
            if self.berth:
                self.berth.start_cycle = self._cycle
                self.berth.total_containers = payload.get("total_containers", 0)
            for hold_info in payload.get("holds", []):
                hold = self.holds.get(hold_info["hold_id"])
                if hold:
                    hold.containers_remaining = hold_info["containers"]
                    hold.hazmat_containers = hold_info.get("hazmat", 0)
                    hold.priority = hold_info.get("priority", "normal")
            self._log_event("vessel_manifest", {"vessel": payload.get("vessel")})
            return {"status": "manifest_loaded", "holds": len(payload.get("holds", []))}

    def handle_entity_discovery(self, payload: dict) -> dict:
        """Register all entities as ACTIVE."""
        with self.lock:
            activated = 0
            for worker in self.workers.values():
                if worker.shift == "A":
                    worker.status = EntityStatus.ACTIVE
                    activated += 1
            for crane in self.cranes.values():
                crane.status = EntityStatus.ACTIVE
                activated += 1
            self._log_event("entity_discovery", {"activated": activated})
            return {"status": "discovered", "activated": activated}

    def handle_capability_matching(self, payload: dict) -> dict:
        """Match requirements against available capabilities."""
        with self.lock:
            requirements = payload.get("requirements", {})
            available = self._count_capabilities()
            gaps = {}
            for req_name, req_count in requirements.items():
                avail_count = available.get(req_name, 0)
                if avail_count < req_count:
                    gaps[req_name] = {"required": req_count, "available": avail_count, "shortfall": req_count - avail_count}
            self._log_event("capability_matching", {"gaps": gaps})
            return {"status": "matched", "gaps": gaps, "available": available}

    def handle_gap_analysis(self, payload: dict) -> dict:
        """Record identified capability gaps."""
        with self.lock:
            gaps = payload.get("gaps", [])
            self._log_event("gap_analysis", {"gap_count": len(gaps), "gaps": gaps})
            return {"status": "analyzed", "gaps": gaps}

    def handle_team_formation(self, payload: dict) -> dict:
        """Form hold teams from available workers."""
        with self.lock:
            for team in payload.get("teams", []):
                hold = self.holds.get(team["hold_id"])
                if not hold:
                    continue
                hold.team_members = team.get("stevedores", [])
                hold.crane_operator = team.get("crane_operator")
                hold.supervisor = team.get("supervisor")
                # Update worker assignments
                all_members = team.get("stevedores", [])
                if team.get("crane_operator"):
                    all_members.append(team["crane_operator"])
                if team.get("supervisor"):
                    all_members.append(team["supervisor"])
                for wid in all_members:
                    worker = self.workers.get(wid)
                    if worker:
                        worker.assigned_hold = team["hold_id"]
            self._log_event("team_formation", {"teams_formed": len(payload.get("teams", []))})
            return {"status": "teams_formed", "count": len(payload.get("teams", []))}

    def handle_recertification_start(self, payload: dict) -> dict:
        """Begin recertification process for workers."""
        with self.lock:
            started = []
            for worker_info in payload.get("workers", []):
                worker = self.workers.get(worker_info["worker_id"])
                if worker:
                    worker.status = EntityStatus.RECERTIFYING
                    target = worker_info.get("target_capability", "hazmat")
                    if target not in worker.capabilities:
                        worker.capabilities[target] = Capability(
                            name=target, status=EntityStatus.RECERTIFYING, cert_valid=False
                        )
                    else:
                        worker.capabilities[target].status = EntityStatus.RECERTIFYING
                    started.append(worker_info["worker_id"])
            self._log_event("recertification_start", {"workers": started})
            return {"status": "recertification_started", "workers": started}

    def handle_capability_update(self, payload: dict) -> dict:
        """Update a single entity's capability."""
        with self.lock:
            entity_id = payload.get("worker_id") or payload.get("entity_id")
            cap_name = payload.get("capability")
            new_status = payload.get("status", "ACTIVE")

            # Worker capability update
            worker = self.workers.get(entity_id)
            if worker:
                if cap_name in worker.capabilities:
                    worker.capabilities[cap_name].status = EntityStatus(new_status)
                    worker.capabilities[cap_name].cert_valid = payload.get("cert_valid", True)
                    worker.capabilities[cap_name].cert_expiry = payload.get("cert_expiry")
                else:
                    worker.capabilities[cap_name] = Capability(
                        name=cap_name,
                        status=EntityStatus(new_status),
                        cert_valid=payload.get("cert_valid", True),
                        cert_expiry=payload.get("cert_expiry"),
                    )
                if new_status == "ACTIVE" and worker.status == EntityStatus.RECERTIFYING:
                    worker.status = EntityStatus.ACTIVE
                self._log_event("capability_update", {"entity": entity_id, "capability": cap_name, "status": new_status})
                return {"status": "updated", "entity": entity_id, "capability": cap_name}

            # Crane capability update
            crane = self.cranes.get(entity_id)
            if crane:
                crane.status = EntityStatus(new_status)
                crane.capacity_pct = payload.get("capacity_pct", 100)
                if new_status == "DEGRADED":
                    crane.moves_per_hour = int(crane.nominal_moves_per_hour * crane.capacity_pct / 100)
                self._log_event("capability_update", {"entity": entity_id, "capability": cap_name, "status": new_status})
                return {"status": "updated", "entity": entity_id, "capability": cap_name}

            return {"status": "error", "message": f"Entity {entity_id} not found"}

    def handle_equipment_fault(self, payload: dict) -> dict:
        """Inject equipment degradation."""
        with self.lock:
            crane = self.cranes.get(payload.get("entity_id"))
            if not crane:
                return {"status": "error", "message": "Crane not found"}
            crane.status = EntityStatus.DEGRADED
            crane.fault_type = payload.get("fault_type")
            crane.moves_per_hour = payload.get("degraded_moves_per_hour", crane.moves_per_hour)
            crane.capacity_pct = int(100 * crane.moves_per_hour / crane.nominal_moves_per_hour)
            self._log_event("equipment_fault", {
                "crane": crane.crane_id,
                "fault": crane.fault_type,
                "degraded_to": crane.moves_per_hour,
            })
            return {"status": "degraded", "crane": crane.crane_id, "moves_per_hour": crane.moves_per_hour}

    def handle_container_redistribution(self, payload: dict) -> dict:
        """Redistribute containers across holds."""
        with self.lock:
            transfers = payload.get("transfers", [])
            for transfer in transfers:
                from_hold = self.holds.get(transfer["from_hold"])
                to_hold = self.holds.get(transfer["to_hold"])
                count = transfer["container_count"]
                if from_hold and to_hold:
                    from_hold.containers_remaining -= count
                    to_hold.containers_remaining += count

            # Update ETAs from revised distribution
            revised = payload.get("revised_distribution", {})
            for hold_id, info in revised.items():
                hold = self.holds.get(hold_id)
                if hold:
                    hold.eta_hours = info.get("eta_hours", 0)

            self._log_event("container_redistribution", {"transfers": len(transfers)})
            return {"status": "redistributed", "transfers": len(transfers)}

    def handle_worker_reassignment(self, payload: dict) -> dict:
        """Reassign workers between holds."""
        with self.lock:
            reassignments = payload.get("reassignments", [])
            moved = []
            for ra in reassignments:
                worker = self.workers.get(ra["worker_id"])
                if not worker:
                    continue
                old_hold = self.holds.get(ra.get("from_hold"))
                new_hold = self.holds.get(ra["to_hold"])
                if old_hold and worker.worker_id in old_hold.team_members:
                    old_hold.team_members.remove(worker.worker_id)
                if new_hold and worker.worker_id not in new_hold.team_members:
                    new_hold.team_members.append(worker.worker_id)
                worker.assigned_hold = ra["to_hold"]
                moved.append(ra["worker_id"])
            self._log_event("worker_reassignment", {"moved": moved})
            return {"status": "reassigned", "moved": moved}

    def handle_batch_status_update(self, payload: dict) -> dict:
        """Batch update worker statuses (e.g., shift change)."""
        with self.lock:
            updates = payload.get("updates", [])
            updated = []
            for update in updates:
                worker = self.workers.get(update["worker_id"])
                if worker:
                    worker.status = EntityStatus(update["status"])
                    if worker.status == EntityStatus.OFFLINE and worker.assigned_hold:
                        hold = self.holds.get(worker.assigned_hold)
                        if hold:
                            if worker.worker_id in hold.team_members:
                                hold.team_members.remove(worker.worker_id)
                            if hold.crane_operator == worker.worker_id:
                                hold.crane_operator = None
                            if hold.supervisor == worker.worker_id:
                                hold.supervisor = None
                        worker.assigned_hold = None
                    updated.append(update["worker_id"])
            self._log_event("batch_status_update", {"updated": updated})
            return {"status": "updated", "count": len(updated)}

    def handle_batch_register(self, payload: dict) -> dict:
        """Batch register new workers (shift arrival)."""
        with self.lock:
            registrations = payload.get("registrations", [])
            registered = []
            for reg in registrations:
                worker = self.workers.get(reg["worker_id"])
                if worker:
                    worker.status = EntityStatus(reg.get("status", "ACTIVE"))
                    registered.append(reg["worker_id"])
            self._log_event("batch_register", {"registered": registered})
            return {"status": "registered", "count": len(registered)}

    def handle_team_reformation(self, payload: dict) -> dict:
        """Reform teams after shift change."""
        return self.handle_team_formation(payload)

    def handle_capacity_alert(self, payload: dict) -> dict:
        """Process hold capacity alert."""
        with self.lock:
            hold = self.holds.get(payload.get("hold_id"))
            if hold:
                hold.eta_hours = payload.get("revised_eta_hours", hold.eta_hours)
            self._log_event("capacity_alert", {
                "hold": payload.get("hold_id"),
                "reduction_pct": payload.get("reduction_pct"),
            })
            return {"status": "alerted", "hold": payload.get("hold_id")}

    def handle_assert_metrics(self, payload: dict) -> dict:
        """Check metric assertions at berth level."""
        with self.lock:
            metrics = self.get_berth_metrics_unlocked()
            assertions = payload.get("assertions", [])
            results = []
            all_passed = True
            for assertion in assertions:
                metric_name = assertion["metric"]
                operator = assertion["operator"]
                expected = assertion["value"]
                actual = metrics.get(metric_name, 0)
                passed = _check_assertion(actual, operator, expected)
                if not passed:
                    all_passed = False
                results.append({
                    "metric": metric_name,
                    "expected": f"{operator} {expected}",
                    "actual": actual,
                    "passed": passed,
                    "description": assertion.get("description", ""),
                })
            self._log_event("assert_metrics", {"all_passed": all_passed, "results": results})
            return {"status": "checked", "all_passed": all_passed, "results": results}

    # --- Query methods ---

    def get_hold_status(self, hold_id: str) -> Optional[dict]:
        with self.lock:
            hold = self.holds.get(hold_id)
            if not hold:
                return None
            crane = self.cranes.get(hold.crane_id)
            return {
                "hold_id": hold.hold_id,
                "containers_remaining": hold.containers_remaining,
                "hazmat_containers": hold.hazmat_containers,
                "crane_status": crane.status.value if crane else "unknown",
                "crane_moves_hr": crane.moves_per_hour if crane else 0,
                "team_size": len(hold.team_members) + (1 if hold.crane_operator else 0) + (1 if hold.supervisor else 0),
                "has_supervisor": hold.supervisor is not None,
                "priority": hold.priority,
                "eta_hours": hold.eta_hours,
            }

    def get_berth_metrics(self) -> dict:
        with self.lock:
            return self.get_berth_metrics_unlocked()

    def get_berth_metrics_unlocked(self) -> dict:
        """Must be called with self.lock held."""
        active_cranes = sum(1 for c in self.cranes.values() if c.status != EntityStatus.OFFLINE)
        active_holds = sum(1 for h in self.holds.values() if self.cranes.get(h.crane_id, Crane("", "", "", 0, 0)).status != EntityStatus.OFFLINE)
        throughput = sum(c.moves_per_hour for c in self.cranes.values() if c.status != EntityStatus.OFFLINE)
        active_workers = sum(1 for w in self.workers.values() if w.status == EntityStatus.ACTIVE)
        total_remaining = sum(h.containers_remaining for h in self.holds.values())

        return {
            "berth_id": self.berth.berth_id if self.berth else "unknown",
            "vessel": self.berth.vessel_name if self.berth else "unknown",
            "active_cranes": active_cranes,
            "active_holds": active_holds,
            "berth_throughput_moves_hr": throughput,
            "active_workers": active_workers,
            "total_containers_remaining": total_remaining,
            "containers_moved": (self.berth.total_containers - total_remaining) if self.berth else 0,
            "workforce_gap_seconds": 0,  # Computed during shift transitions
        }

    def get_all_state(self) -> dict:
        """Full state snapshot for debugging/logging."""
        with self.lock:
            return {
                "cycle": self._cycle,
                "berth": asdict(self.berth) if self.berth else None,
                "cranes": {cid: asdict(c) for cid, c in self.cranes.items()},
                "workers": {
                    wid: {
                        "worker_id": w.worker_id,
                        "shift": w.shift,
                        "status": w.status.value,
                        "assigned_hold": w.assigned_hold,
                        "capabilities": {
                            cn: {"status": cv.status.value, "cert_valid": cv.cert_valid}
                            for cn, cv in w.capabilities.items()
                        },
                    }
                    for wid, w in self.workers.items()
                },
                "holds": {
                    hid: {
                        "hold_id": h.hold_id,
                        "containers_remaining": h.containers_remaining,
                        "hazmat_containers": h.hazmat_containers,
                        "team_members": h.team_members,
                        "crane_operator": h.crane_operator,
                        "supervisor": h.supervisor,
                        "eta_hours": h.eta_hours,
                    }
                    for hid, h in self.holds.items()
                },
                "metrics": self.get_berth_metrics_unlocked(),
            }

    def _count_capabilities(self) -> dict:
        """Count active capabilities across all workers."""
        counts = defaultdict(int)
        for worker in self.workers.values():
            if worker.status not in (EntityStatus.ACTIVE, EntityStatus.RECERTIFYING):
                continue
            for cap_name, cap in worker.capabilities.items():
                if cap.status == EntityStatus.ACTIVE and cap.cert_valid:
                    # Map capability names to requirement names
                    if cap_name == "hazmat":
                        counts["hazmat_certified_workers"] += 1
                    elif cap_name == "crane-operator":
                        counts["crane_operators"] += 1
                    elif cap_name == "stevedore":
                        counts["stevedores"] += 1
                    elif cap_name == "lashing":
                        counts["lashing_crew"] += 1
                    elif cap_name == "supervisor":
                        counts["supervisors"] += 1
        return dict(counts)


# --- Event dispatch ---

EVENT_HANDLERS = {
    "vessel_manifest": "handle_vessel_manifest",
    "entity_discovery": "handle_entity_discovery",
    "capability_matching": "handle_capability_matching",
    "gap_analysis": "handle_gap_analysis",
    "team_formation": "handle_team_formation",
    "recertification_start": "handle_recertification_start",
    "capability_update": "handle_capability_update",
    "capability_assertion": "handle_capability_update",
    "content_routing": "handle_recertification_start",  # Triggers same flow
    "equipment_fault": "handle_equipment_fault",
    "hold_capacity_alert": "handle_capacity_alert",
    "container_redistribution": "handle_container_redistribution",
    "worker_redistribution": "handle_worker_reassignment",
    "worker_reassignment": "handle_worker_reassignment",
    "shift_transition_start": "handle_batch_status_update",  # Noop start signal
    "worker_status_batch": "handle_batch_status_update",
    "worker_registration_batch": "handle_batch_register",
    "team_reformation": "handle_team_reformation",
    "berth_metrics_check": "handle_assert_metrics",
}


def dispatch_event(manager: "EntityStateManager", event: dict) -> dict:
    """Dispatch a scenario event to the appropriate handler."""
    event_type = event.get("type", "")
    handler_name = EVENT_HANDLERS.get(event_type)
    if not handler_name:
        return {"status": "error", "message": f"Unknown event type: {event_type}"}
    handler = getattr(manager, handler_name, None)
    if not handler:
        return {"status": "error", "message": f"Handler not found: {handler_name}"}
    payload = event.get("payload", {})
    return handler(payload)


def _check_assertion(actual, operator: str, expected) -> bool:
    if operator == ">=":
        return actual >= expected
    elif operator == "<=":
        return actual <= expected
    elif operator == "==":
        return actual == expected
    elif operator == ">":
        return actual > expected
    elif operator == "<":
        return actual < expected
    return False
