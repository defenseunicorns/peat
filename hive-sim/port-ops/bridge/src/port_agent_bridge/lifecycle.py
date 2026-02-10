"""
Capability Lifecycle Engine — degradation, resources, certification, logistics, gap analysis.

Runs alongside the OODA loop, emitting lifecycle events as JSON lines to stdout.
The Rust relay classifies any {"event_type": ...} as HiveEvent and forwards it.

Usage:
    mgr = LifecycleManager(node_id="crane-1")
    events = mgr.tick(cycle=1, action="complete_container_move",
                      node_id="crane-1", sim_minutes=2.5)
    for evt in events:
        print(json.dumps(evt), flush=True)
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Any


# ---------------------------------------------------------------------------
#  Degradation Tracker
# ---------------------------------------------------------------------------

# Subsystem decay rates per action (confidence loss per action)
SUBSYSTEM_DECAY: dict[str, float] = {
    "hydraulic":  0.035,   # fast decay — heavy mechanical stress
    "spreader":   0.020,   # medium — alignment wears gradually
    "electrical": 0.010,   # slow — solid state, rare faults
}

# Health thresholds (matches ADR-053 / types.ts)
NOMINAL_THRESHOLD   = 0.7
DEGRADED_THRESHOLD  = 0.4


def _health_status(confidence: float) -> str:
    if confidence <= 0:
        return "OFFLINE"
    if confidence < DEGRADED_THRESHOLD:
        return "CRITICAL"
    if confidence < NOMINAL_THRESHOLD:
        return "DEGRADED"
    return "NOMINAL"


def _priority_for_status(status: str) -> str:
    return {
        "NOMINAL": "ROUTINE",
        "DEGRADED": "HIGH",
        "CRITICAL": "CRITICAL",
        "OFFLINE": "CRITICAL",
    }.get(status, "ROUTINE")


class DegradationTracker:
    """Per-equipment confidence decay based on action count."""

    def __init__(self, node_id: str, subsystems: dict[str, float] | None = None):
        self.node_id = node_id
        self._decay_rates = dict(subsystems) if subsystems else dict(SUBSYSTEM_DECAY)
        # Per-subsystem confidence [0.0, 1.0]
        self.subsystems: dict[str, float] = {
            k: 1.0 for k in self._decay_rates
        }

    @property
    def overall_confidence(self) -> float:
        vals = self.subsystems.values()
        return min(vals) if vals else 1.0

    def tick(self, action: str, cycle: int, physical_actions: set[str] | None = None) -> list[dict]:
        """Apply decay for an action, return degradation events."""
        _physical = physical_actions or {"complete_container_move", "report_equipment_status"}
        if action not in _physical:
            return []

        events: list[dict] = []
        for subsystem, rate in self._decay_rates.items():
            before = self.subsystems[subsystem]
            after = max(0.0, before - rate)
            self.subsystems[subsystem] = after

            status_before = _health_status(before)
            status_after = _health_status(after)

            # Emit on threshold crossing or every 5 cycles
            if status_before != status_after or cycle % 5 == 0:
                events.append({
                    "event_type": "CAPABILITY_DEGRADED",
                    "source": self.node_id,
                    "priority": _priority_for_status(status_after),
                    "details": {
                        "subsystem": subsystem,
                        "before": round(before, 4),
                        "after": round(after, 4),
                        "status": status_after,
                        "cause": f"lift_cycle_{cycle}",
                    },
                })
        return events

    def restore(self, subsystem: str, amount: float) -> None:
        """Restore confidence after maintenance."""
        if subsystem in self.subsystems:
            self.subsystems[subsystem] = min(1.0, self.subsystems[subsystem] + amount)


# ---------------------------------------------------------------------------
#  Resource Tracker
# ---------------------------------------------------------------------------

@dataclass
class ResourceLevel:
    name: str
    value: float           # 0-100 percent
    consume_per_action: float
    warning_threshold: float = 25.0
    critical_threshold: float = 10.0


class ResourceTracker:
    """Per-equipment resource levels (battery, hydraulic fluid, fuel)."""

    def __init__(self, node_id: str, resources: dict[str, dict] | None = None):
        self.node_id = node_id
        if resources:
            self.resources: dict[str, ResourceLevel] = {
                name: ResourceLevel(
                    name,
                    r.get("value", 100.0),
                    r.get("drain", 1.0),
                    r.get("warning", 25.0),
                    r.get("critical", 10.0),
                )
                for name, r in resources.items()
            }
        else:
            self.resources = {
                "hydraulic_fluid_pct": ResourceLevel("hydraulic_fluid_pct", 100.0, 2.5, 25.0, 10.0),
                "battery_pct":         ResourceLevel("battery_pct", 100.0, 1.0, 20.0, 5.0),
                "fuel_pct":            ResourceLevel("fuel_pct", 100.0, 1.5, 30.0, 15.0),
            }
        self.state = "OPERATIONAL"  # OPERATIONAL | RESUPPLYING
        self._resupply_complete_at: float | None = None  # sim_minutes when resupply finishes

    def tick(self, action: str, sim_minutes: float, physical_actions: set[str] | None = None) -> list[dict]:
        """Consume resources for an action, return events."""
        events: list[dict] = []

        # Check if resupply completed
        if self._resupply_complete_at and sim_minutes >= self._resupply_complete_at:
            self._resupply_complete_at = None
            self.state = "OPERATIONAL"
            for r in self.resources.values():
                r.value = 100.0
            events.append({
                "event_type": "RESUPPLY_COMPLETED",
                "source": self.node_id,
                "priority": "HIGH",
                "details": {
                    "equipment_id": self.node_id,
                    "state": "OPERATIONAL",
                },
            })
            return events

        _physical = physical_actions or {"complete_container_move", "report_equipment_status"}
        if action not in _physical:
            return events

        for r in self.resources.values():
            before = r.value
            r.value = max(0.0, r.value - r.consume_per_action)

            events.append({
                "event_type": "RESOURCE_CONSUMED",
                "source": self.node_id,
                "priority": "ROUTINE",
                "details": {
                    "resource": r.name,
                    "before": round(before, 1),
                    "after": round(r.value, 1),
                },
            })

            # Trigger resupply when any resource hits warning threshold
            if before >= r.warning_threshold > r.value and self.state == "OPERATIONAL":
                self.state = "RESUPPLYING"
                self._resupply_complete_at = sim_minutes + 5.0  # 5 sim-minutes
                events.append({
                    "event_type": "RESUPPLY_REQUESTED",
                    "source": self.node_id,
                    "priority": "HIGH",
                    "details": {
                        "resource": r.name,
                        "current_level": round(r.value, 1),
                        "equipment_id": self.node_id,
                        "eta_sim_minutes": 5.0,
                    },
                })

        return events


# ---------------------------------------------------------------------------
#  Certification Tracker
# ---------------------------------------------------------------------------

# Proficiency → recertification time multiplier (experts recertify faster)
_RECERT_TIME_MULTIPLIER: dict[str, float] = {
    "expert":            0.5,    # 5 sim-minutes (fast)
    "competent":         0.75,   # 7.5 sim-minutes
    "advanced_beginner": 1.0,    # 10 sim-minutes (baseline)
    "novice":            1.5,    # 15 sim-minutes (slow)
}


class CertTracker:
    """Per-worker certification expiry based on simulated hours."""

    def __init__(self, node_id: str, cert_hours: float = 120.0, proficiency: str = "competent"):
        self.node_id = node_id
        self.proficiency = proficiency
        self.cert_hours_remaining = cert_hours  # hours until expiry
        self.warning_hours = 20.0
        self.expired = False
        self._recert_complete_at: float | None = None
        self._recert_base_minutes = 10.0
        self._recert_multiplier = _RECERT_TIME_MULTIPLIER.get(proficiency, 1.0)

    def tick(self, sim_minutes: float) -> list[dict]:
        """Advance cert clock, return expiry events."""
        events: list[dict] = []

        # Check recertification completion
        if self._recert_complete_at and sim_minutes >= self._recert_complete_at:
            self._recert_complete_at = None
            self.expired = False
            self.cert_hours_remaining = 120.0
            events.append({
                "event_type": "RECERTIFICATION_COMPLETED",
                "source": self.node_id,
                "priority": "HIGH",
                "details": {
                    "worker_id": self.node_id,
                    "new_expiry_hours": 120.0,
                    "proficiency": self.proficiency,
                },
            })
            return events

        if self.expired:
            return events

        hours_elapsed = sim_minutes / 60.0
        self.cert_hours_remaining -= hours_elapsed

        if self.cert_hours_remaining <= 0:
            self.cert_hours_remaining = 0
            self.expired = True
            recert_time = self._recert_base_minutes * self._recert_multiplier
            self._recert_complete_at = sim_minutes + recert_time
            events.append({
                "event_type": "CERTIFICATION_EXPIRED",
                "source": self.node_id,
                "priority": "CRITICAL",
                "details": {
                    "worker_id": self.node_id,
                    "proficiency": self.proficiency,
                    "recert_eta_sim_minutes": recert_time,
                },
            })
        elif self.cert_hours_remaining <= self.warning_hours:
            events.append({
                "event_type": "CERTIFICATION_EXPIRING",
                "source": self.node_id,
                "priority": "HIGH",
                "details": {
                    "worker_id": self.node_id,
                    "proficiency": self.proficiency,
                    "hours_remaining": round(self.cert_hours_remaining, 1),
                },
            })

        return events


# ---------------------------------------------------------------------------
#  Logistics Dispatcher
# ---------------------------------------------------------------------------

@dataclass
class MaintenanceJob:
    equipment_id: str
    subsystem: str
    scheduled_at: float   # sim_minutes when scheduled
    start_at: float       # sim_minutes when crew arrives
    complete_at: float    # sim_minutes when done
    started: bool = False
    completed: bool = False


class LogisticsDispatcher:
    """Orchestrates maintenance/resupply/shift relief."""

    def __init__(self, node_id: str):
        self.node_id = node_id
        self.jobs: list[MaintenanceJob] = []
        self._shift_relief_requested = False
        self._shift_relief_at: float | None = None

    def check_degradation(
        self, degradation: DegradationTracker, sim_minutes: float
    ) -> list[dict]:
        """Watch for degradation below thresholds, dispatch maintenance."""
        events: list[dict] = []

        for subsystem, confidence in degradation.subsystems.items():
            status = _health_status(confidence)
            if status in ("CRITICAL", "OFFLINE"):
                # Check if we already have a job for this subsystem
                existing = any(
                    j.subsystem == subsystem and not j.completed
                    for j in self.jobs
                )
                if not existing:
                    job = MaintenanceJob(
                        equipment_id=self.node_id,
                        subsystem=subsystem,
                        scheduled_at=sim_minutes,
                        start_at=sim_minutes + 2.0,
                        complete_at=sim_minutes + 8.0,
                    )
                    self.jobs.append(job)
                    events.append({
                        "event_type": "MAINTENANCE_SCHEDULED",
                        "source": self.node_id,
                        "priority": "HIGH",
                        "details": {
                            "equipment_id": self.node_id,
                            "subsystem": subsystem,
                            "reason": f"{subsystem}_critical",
                            "eta_start_secs": 120,
                            "eta_complete_secs": 480,
                        },
                    })
        return events

    def tick(
        self, sim_minutes: float, degradation: DegradationTracker
    ) -> list[dict]:
        """Progress maintenance jobs and shift relief."""
        events: list[dict] = []

        for job in self.jobs:
            if job.completed:
                continue

            if not job.started and sim_minutes >= job.start_at:
                job.started = True
                events.append({
                    "event_type": "MAINTENANCE_STARTED",
                    "source": self.node_id,
                    "priority": "HIGH",
                    "details": {
                        "equipment_id": job.equipment_id,
                        "subsystem": job.subsystem,
                    },
                })

            if job.started and not job.completed and sim_minutes >= job.complete_at:
                job.completed = True
                degradation.restore(job.subsystem, 0.5)
                events.append({
                    "event_type": "MAINTENANCE_COMPLETE",
                    "source": self.node_id,
                    "priority": "HIGH",
                    "details": {
                        "equipment_id": job.equipment_id,
                        "subsystem": job.subsystem,
                        "restored_confidence": round(
                            degradation.subsystems.get(job.subsystem, 0), 4
                        ),
                    },
                })

        # Shift relief: request after 60 sim-minutes
        if not self._shift_relief_requested and sim_minutes >= 60.0:
            self._shift_relief_requested = True
            self._shift_relief_at = sim_minutes + 5.0
            events.append({
                "event_type": "SHIFT_RELIEF_REQUESTED",
                "source": self.node_id,
                "priority": "HIGH",
                "details": {
                    "equipment_id": self.node_id,
                    "reason": "shift_end",
                    "eta_sim_minutes": 5.0,
                },
            })

        if self._shift_relief_at and sim_minutes >= self._shift_relief_at:
            self._shift_relief_at = None
            events.append({
                "event_type": "SHIFT_RELIEF_ARRIVED",
                "source": self.node_id,
                "priority": "HIGH",
                "details": {"equipment_id": self.node_id},
            })

        return events


# ---------------------------------------------------------------------------
#  Gap Analyzer
# ---------------------------------------------------------------------------

class GapAnalyzer:
    """Periodic gap analysis reports for hold-level aggregation."""

    def __init__(self, node_id: str, report_every_n_cycles: int = 10):
        self.node_id = node_id
        self.report_every = report_every_n_cycles

    def tick(
        self,
        cycle: int,
        degradation: DegradationTracker,
        resources: ResourceTracker,
        logistics: LogisticsDispatcher,
    ) -> list[dict]:
        """Emit gap analysis report every N cycles."""
        if cycle % self.report_every != 0 or cycle == 0:
            return []

        gaps = []
        for subsystem, confidence in degradation.subsystems.items():
            status = _health_status(confidence)
            if status != "NOMINAL":
                pending = []
                for job in logistics.jobs:
                    if job.subsystem == subsystem and not job.completed:
                        pending.append({
                            "id": f"maint-{job.equipment_id}-{subsystem}",
                            "description": f"Maintain {subsystem} on {job.equipment_id}",
                            "eta_minutes": max(0, round(job.complete_at - (cycle * 1.5), 1)),
                            "status": "in_progress" if job.started else "pending",
                            "blocked_by": None,
                        })
                gaps.append({
                    "capability_name": subsystem.upper(),
                    "capability_type": "payload",
                    "required_confidence": NOMINAL_THRESHOLD,
                    "current_confidence": round(confidence, 4),
                    "decay_rate": -degradation._decay_rates.get(subsystem, 0.01),
                    "status": status,
                    "pending_actions": pending,
                })

        # Resource gaps
        for r in resources.resources.values():
            if r.value < r.warning_threshold:
                gaps.append({
                    "capability_name": r.name.upper(),
                    "capability_type": "payload",
                    "required_confidence": r.warning_threshold / 100.0,
                    "current_confidence": round(r.value / 100.0, 4),
                    "decay_rate": -r.consume_per_action / 100.0,
                    "status": "CRITICAL" if r.value < r.critical_threshold else "DEGRADED",
                    "pending_actions": [],
                })

        # Readiness: average of all subsystem confidences
        all_conf = list(degradation.subsystems.values())
        readiness = sum(all_conf) / len(all_conf) if all_conf else 1.0

        return [{
            "event_type": "GAP_ANALYSIS_REPORT",
            "source": self.node_id,
            "priority": "HIGH" if gaps else "ROUTINE",
            "details": {
                "level": "H2",
                "location_id": self.node_id,
                "readiness_score": round(readiness, 4),
                "gaps": gaps,
                "pending_jobs": len([j for j in logistics.jobs if not j.completed]),
                "resource_state": resources.state,
            },
        }]


# ---------------------------------------------------------------------------
#  Lifecycle Manager (facade)
# ---------------------------------------------------------------------------

# ── Role-specific lifecycle configurations ─────────────────────────────

_ROLE_CONFIGS: dict[str, dict] = {
    "crane": {
        "subsystems": {"hydraulic": 0.035, "spreader": 0.020, "electrical": 0.010},
        "resources": None,  # use defaults
        "cert_hours": 120.0,
        "physical_actions": {"complete_container_move", "report_equipment_status"},
    },
    "tractor": {
        "subsystems": {"battery": 0.060, "drivetrain": 0.015, "hydraulic_lift": 0.025},
        "resources": {"battery_pct": {"drain": 5.0, "warning": 30.0, "critical": 10.0}},
        "cert_hours": None,
        "physical_actions": {"transport_container", "report_equipment_status"},
    },
    "operator": {
        "subsystems": None,
        "resources": None,
        "cert_hours": 60.0,  # faster expiry for drama
        "physical_actions": set(),
    },
    "sensor": {
        "subsystems": {"calibration": 0.008},
        "resources": {"power_pct": {"drain": 0.5, "warning": 20.0, "critical": 5.0}},
        "cert_hours": None,
        "physical_actions": {"emit_reading", "report_calibration"},
    },
    # scheduler/aggregator/berth_manager: no lifecycle
    "berth_manager": {"subsystems": None, "resources": None, "cert_hours": None, "physical_actions": set()},
}


class LifecycleManager:
    """
    Facade that owns all lifecycle trackers for one equipment node.

    Called once per OODA cycle from the loop.
    Returns list of JSON event dicts to print to stdout.
    """

    def __init__(self, node_id: str, role: str = "crane", report_every_n_cycles: int = 10, proficiency: str = "competent"):
        self.node_id = node_id
        self.role = role
        self.proficiency = proficiency
        cfg = _ROLE_CONFIGS.get(role, {})
        self._physical_actions: set[str] = cfg.get("physical_actions", {"complete_container_move", "report_equipment_status"})

        # Degradation
        subsystems = cfg.get("subsystems")
        self.degradation = DegradationTracker(node_id, subsystems) if subsystems else None

        # Resources
        resources = cfg.get("resources")
        self.resources = ResourceTracker(node_id, resources) if resources is not None or role == "crane" else None
        if role == "crane" and resources is None:
            self.resources = ResourceTracker(node_id)

        # Certifications (operators and cranes) — proficiency affects renewal time
        cert_hours = cfg.get("cert_hours")
        self.certs = CertTracker(node_id, cert_hours=cert_hours, proficiency=proficiency) if cert_hours else None

        # Logistics + gap analysis (only if degradation is tracked)
        self.logistics = LogisticsDispatcher(node_id) if self.degradation else None
        self.gap_analyzer = GapAnalyzer(node_id, report_every_n_cycles) if self.degradation else None
        self._last_sim_minutes = 0.0

    def tick(
        self,
        cycle: int,
        action: str,
        node_id: str,
        sim_minutes: float,
    ) -> list[dict]:
        """
        Run all lifecycle trackers for this cycle.

        Returns list of event dicts (each has event_type, source, priority, details).
        """
        # Roles without lifecycle: return nothing
        if self.role in ("scheduler", "aggregator", "berth_manager") and not self.degradation and not self.certs:
            return []

        events: list[dict] = []

        # Delta sim time for cert tracker
        delta_minutes = sim_minutes - self._last_sim_minutes
        self._last_sim_minutes = sim_minutes

        # 1. Degradation
        if self.degradation:
            events.extend(self.degradation.tick(action, cycle, self._physical_actions))

        # 2. Resources
        if self.resources:
            events.extend(self.resources.tick(action, sim_minutes, self._physical_actions))

        # 3. Certifications
        if self.certs:
            events.extend(self.certs.tick(delta_minutes))

        # 4. Logistics: check degradation → schedule maintenance
        if self.logistics and self.degradation:
            events.extend(
                self.logistics.check_degradation(self.degradation, sim_minutes)
            )

        # 5. Logistics: progress jobs
        if self.logistics and self.degradation:
            events.extend(self.logistics.tick(sim_minutes, self.degradation))

        # 6. Gap analysis
        if self.gap_analyzer and self.degradation and self.resources and self.logistics:
            events.extend(
                self.gap_analyzer.tick(
                    cycle, self.degradation, self.resources, self.logistics
                )
            )

        # Stamp all events
        ts_us = int(time.time() * 1_000_000)
        for evt in events:
            evt.setdefault("timestamp_us", ts_us)

        return events
