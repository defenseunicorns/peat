"""
LLM decision functions for the port terminal simulation.

Each role has a ``_decide_<role>()`` function that transforms the entity's
current view of the world into a set of tool calls (actions).  The top-level
``decide()`` dispatcher is wired into ``Orchestrator.tick()``.
"""

from __future__ import annotations

import statistics
from typing import Any

from .orchestrator import Entity, Orchestrator


# ---------------------------------------------------------------------------
# Dispatcher
# ---------------------------------------------------------------------------

_DECISION_FUNCTIONS: dict[str, Any] = {}


def _register(role: str):
    """Decorator that registers a decision function for a role."""
    def decorator(fn):
        _DECISION_FUNCTIONS[role] = fn
        return fn
    return decorator


def decide(entity: Entity, orchestrator: Orchestrator) -> dict[str, Any]:
    """Route an entity to its role-specific decision function."""
    fn = _DECISION_FUNCTIONS.get(entity.role)
    if fn is None:
        return {"error": f"no decision function for role '{entity.role}'"}
    return fn(entity, orchestrator)


# ---------------------------------------------------------------------------
# Yard Manager (H3)
# ---------------------------------------------------------------------------

# Thresholds (match persona decision framework)
_CONGESTION_QUEUE_DEPTH = 3
_UTILIZATION_BALANCE_STDDEV = 0.15
_ESCALATION_UTILIZATION = 0.85
_ESCALATION_CAPACITY_DROP = 0.20


@_register("yard_manager")
def _decide_yard_manager(entity: Entity, orchestrator: Orchestrator) -> dict[str, Any]:
    """Yard Manager decision cycle.

    Steps:
      1. Aggregate yard block statuses from subordinates
      2. Detect and mitigate congestion
      3. Optimize tractor routing for pending assignments
      4. Assign stacking crane tasks
      5. Escalate unresolvable issues to TOC
    """
    actions: list[dict[str, Any]] = []

    # -- 1. Aggregate yard block statuses ------------------------------------
    block_summaries = _gather_block_summaries(entity, orchestrator)
    yard_summary = _aggregate_yard_summary(entity, block_summaries)
    actions.append({
        "tool": "update_yard_summary",
        "params": yard_summary,
    })

    # -- 2. Congestion detection & mitigation --------------------------------
    congested = [
        b for b in block_summaries
        if b.get("queue_depth", 0) > _CONGESTION_QUEUE_DEPTH
    ]
    for block in congested:
        actions.append({
            "tool": "report_congestion",
            "params": {
                "block_id": block["block_id"],
                "queue_depth": block["queue_depth"],
                "zone": entity.zone_scope,
            },
        })

    # -- 3. Tractor routing --------------------------------------------------
    pending_tractors = entity.state.get("pending_tractors", [])
    for tractor in pending_tractors:
        best_block = _pick_best_block(block_summaries, tractor, congested)
        if best_block is not None:
            actions.append({
                "tool": "route_tractor",
                "params": {
                    "tractor_id": tractor["tractor_id"],
                    "target_block": best_block["block_id"],
                    "container_id": tractor.get("container_id"),
                },
            })
            actions.append({
                "tool": "assign_yard_block",
                "params": {
                    "container_id": tractor.get("container_id"),
                    "block_id": best_block["block_id"],
                },
            })

    # -- 4. Stacking crane balancing -----------------------------------------
    _balance_cranes(block_summaries, actions)

    # -- 5. Escalation -------------------------------------------------------
    _maybe_escalate(entity, yard_summary, congested, actions)

    return {"actions": actions, "summary": yard_summary}


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

def _gather_block_summaries(
    entity: Entity,
    orchestrator: Orchestrator,
) -> list[dict[str, Any]]:
    """Read yard-block (H2) subordinate state."""
    summaries: list[dict[str, Any]] = []
    for sub in orchestrator.subordinates_of(entity.entity_id):
        summary = sub.state.get("block_summary", {})
        summary.setdefault("block_id", sub.entity_id)
        summaries.append(summary)
    return summaries


def _aggregate_yard_summary(
    entity: Entity,
    blocks: list[dict[str, Any]],
) -> dict[str, Any]:
    """Build a consolidated yard zone summary."""
    total_capacity = sum(b.get("capacity_teu", 0) for b in blocks)
    total_used = sum(b.get("used_teu", 0) for b in blocks)
    utilization = total_used / total_capacity if total_capacity else 0.0

    utils = [
        b.get("used_teu", 0) / b.get("capacity_teu", 1) for b in blocks
    ]
    balance_stddev = statistics.pstdev(utils) if len(utils) > 1 else 0.0

    return {
        "zone": entity.zone_scope,
        "block_count": len(blocks),
        "total_capacity_teu": total_capacity,
        "total_used_teu": total_used,
        "utilization": round(utilization, 3),
        "balance_stddev": round(balance_stddev, 3),
        "reefer_slots_free": sum(b.get("reefer_free", 0) for b in blocks),
        "hazmat_zones_active": sum(1 for b in blocks if b.get("hazmat_active")),
    }


def _pick_best_block(
    blocks: list[dict[str, Any]],
    tractor: dict[str, Any],
    congested: list[dict[str, Any]],
) -> dict[str, Any] | None:
    """Pick the optimal yard block for an inbound tractor.

    Selection factors (priority order):
      1. Skip congested blocks
      2. Container type compatibility (reefer, hazmat)
      3. Lowest utilization
    """
    congested_ids = {b["block_id"] for b in congested}
    container_type = tractor.get("container_type", "dry")

    candidates = [b for b in blocks if b["block_id"] not in congested_ids]

    if container_type == "reefer":
        candidates = [b for b in candidates if b.get("reefer_free", 0) > 0]
    elif container_type == "hazmat":
        candidates = [b for b in candidates if b.get("hazmat_capable")]

    if not candidates:
        return None

    # Lowest utilization wins
    candidates.sort(
        key=lambda b: b.get("used_teu", 0) / max(b.get("capacity_teu", 1), 1)
    )
    return candidates[0]


def _balance_cranes(
    blocks: list[dict[str, Any]],
    actions: list[dict[str, Any]],
) -> None:
    """Detect crane imbalance and generate rebalancing assignments."""
    crane_loads = [
        (b["block_id"], b.get("crane_queue", 0), b.get("crane_idle", 0))
        for b in blocks
    ]
    overloaded = [(bid, q, idle) for bid, q, idle in crane_loads if q > 5 and idle == 0]
    underloaded = [(bid, q, idle) for bid, q, idle in crane_loads if idle > 0]

    for over_bid, over_q, _ in overloaded:
        for under_bid, _, under_idle in underloaded:
            if under_idle > 0:
                actions.append({
                    "tool": "assign_yard_block",
                    "params": {
                        "action": "rebalance_crane",
                        "from_block": under_bid,
                        "to_block": over_bid,
                    },
                })
                break



# ---------------------------------------------------------------------------
# Stacking Crane (H1)
# ---------------------------------------------------------------------------

# Operational limits
_HOIST_LOAD_MAX_KG = 40_000
_HOIST_LOAD_WARN_KG = 38_000  # 95% of rated
_MAX_TIER = 5  # max container stack height


@_register("stacking_crane")
def _decide_stacking_crane(entity: Entity, orchestrator: Orchestrator) -> dict[str, Any]:
    """Stacking crane decision cycle (dry-run).

    Steps:
      1. Check subsystem health
      2. If current task: execute next step (receive -> travel -> stack/retrieve -> report)
      3. If idle: pull next task from yard block queue
    """
    actions: list[dict[str, Any]] = []
    task = entity.state.get("current_task")

    # -- 1. Subsystem health check ------------------------------------------
    hoist_load = entity.state.get("hoist_load_kg", 0.0)
    if hoist_load > _HOIST_LOAD_MAX_KG:
        actions.append({
            "tool": "report_position",
            "params": {
                "crane_id": entity.entity_id,
                "fault": "hoist_overload",
                "load_kg": hoist_load,
                "yard_block": entity.state.get("yard_block"),
            },
        })
        return {"actions": actions, "status": "fault"}

    # -- 2. Execute current task --------------------------------------------
    if task is not None:
        _execute_crane_task(entity, task, actions)
        return {"actions": actions, "status": "working"}

    # -- 3. Pull next task from queue ---------------------------------------
    task_queue = entity.state.get("task_queue", [])
    if task_queue:
        next_task = task_queue.pop(0)
        entity.state["current_task"] = next_task
        _execute_crane_task(entity, next_task, actions)
        return {"actions": actions, "status": "working"}

    # Idle — report position
    actions.append({
        "tool": "report_position",
        "params": {
            "crane_id": entity.entity_id,
            "status": "idle",
            "position": entity.state.get("position", {}),
            "yard_block": entity.state.get("yard_block"),
        },
    })
    return {"actions": actions, "status": "idle"}


def _execute_crane_task(
    entity: Entity,
    task: dict[str, Any],
    actions: list[dict[str, Any]],
) -> None:
    """Progress through a stack or retrieve task."""
    task_type = task.get("type", "stack")
    phase = task.get("phase", "receive")

    if task_type == "stack":
        _execute_stack(entity, task, phase, actions)
    elif task_type == "retrieve":
        _execute_retrieve(entity, task, phase, actions)


def _execute_stack(
    entity: Entity,
    task: dict[str, Any],
    phase: str,
    actions: list[dict[str, Any]],
) -> None:
    """Stack sequence: receive -> travel -> place -> complete."""
    container_id = task.get("container_id")
    target = task.get("target_slot", {})

    if phase == "receive":
        # Receive container from tractor at transfer lane
        entity.state["hoist_load_kg"] = task.get("weight_kg", 20_000)
        task["phase"] = "travel"
        actions.append({
            "tool": "report_position",
            "params": {
                "crane_id": entity.entity_id,
                "status": "received",
                "container_id": container_id,
                "yard_block": entity.state.get("yard_block"),
            },
        })

    elif phase == "travel":
        # Move gantry + trolley to target slot
        entity.state["position"] = {
            "row": target.get("row", 0),
            "bay": target.get("bay", 0),
        }
        task["phase"] = "place"

    elif phase == "place":
        # Lower container into slot
        tier = target.get("tier", 0)
        if tier > _MAX_TIER:
            actions.append({
                "tool": "report_position",
                "params": {
                    "crane_id": entity.entity_id,
                    "fault": "tier_limit_exceeded",
                    "container_id": container_id,
                    "target_slot": target,
                    "yard_block": entity.state.get("yard_block"),
                },
            })
            entity.state["current_task"] = None
            return
        actions.append({
            "tool": "stack_container",
            "params": {
                "crane_id": entity.entity_id,
                "container_id": container_id,
                "row": target.get("row", 0),
                "bay": target.get("bay", 0),
                "tier": tier,
                "yard_block": entity.state.get("yard_block"),
            },
        })
        entity.state["hoist_load_kg"] = 0.0
        entity.state["current_task"] = None


def _execute_retrieve(
    entity: Entity,
    task: dict[str, Any],
    phase: str,
    actions: list[dict[str, Any]],
) -> None:
    """Retrieve sequence: travel -> pick -> deliver -> complete."""
    container_id = task.get("container_id")
    source = task.get("source_slot", {})

    if phase == "receive":
        # 'receive' means accept the task — first step is travel
        task["phase"] = "travel"

    elif phase == "travel":
        entity.state["position"] = {
            "row": source.get("row", 0),
            "bay": source.get("bay", 0),
        }
        task["phase"] = "pick"

    elif phase == "pick":
        entity.state["hoist_load_kg"] = task.get("weight_kg", 20_000)
        actions.append({
            "tool": "retrieve_container",
            "params": {
                "crane_id": entity.entity_id,
                "container_id": container_id,
                "row": source.get("row", 0),
                "bay": source.get("bay", 0),
                "tier": source.get("tier", 0),
                "yard_block": entity.state.get("yard_block"),
            },
        })
        task["phase"] = "deliver"

    elif phase == "deliver":
        # Move to transfer lane and lower onto tractor
        entity.state["position"] = {"row": 0, "bay": 0}
        entity.state["hoist_load_kg"] = 0.0
        entity.state["current_task"] = None
        actions.append({
            "tool": "report_position",
            "params": {
                "crane_id": entity.entity_id,
                "status": "delivered",
                "container_id": container_id,
                "yard_block": entity.state.get("yard_block"),
            },
        })


def _maybe_escalate(
    entity: Entity,
    yard_summary: dict[str, Any],
    congested: list[dict[str, Any]],
    actions: list[dict[str, Any]],
) -> None:
    """Escalate to TOC when yard-level thresholds are breached."""
    reasons: list[str] = []

    if yard_summary["utilization"] > _ESCALATION_UTILIZATION:
        reasons.append(
            f"yard utilization {yard_summary['utilization']:.0%} exceeds {_ESCALATION_UTILIZATION:.0%}"
        )

    stale_congestion = entity.state.get("congestion_cycles", 0)
    if congested and stale_congestion >= 2:
        reasons.append(
            f"congestion persists after {stale_congestion} mitigation cycles "
            f"({len(congested)} blocks)"
        )

    if reasons:
        actions.append({
            "tool": "report_congestion",
            "params": {
                "escalate_to": "toc",
                "zone": entity.zone_scope,
                "reasons": reasons,
            },
        })
