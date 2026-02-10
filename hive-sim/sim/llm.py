"""LLM decision module for sim agents.

Provides tiered decision routing per Addendum A, Option C (Tiered):
- rule_based: Deterministic state machine, no LLM calls (sensors)
- llm: Full LLM reasoning (equipment agents, human proxies)

Sensor agents (load cells, RFID readers, temperature/pressure probes)
follow deterministic rules: read calibration state, emit readings at
fixed intervals, detect anomalies via threshold comparison.
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from enum import Enum
from typing import TYPE_CHECKING, Any, Protocol

if TYPE_CHECKING:
    from .orchestrator import Entity


class SensorState(str, Enum):
    """State machine states for rule-based sensor agents."""

    CALIBRATING = "calibrating"
    NOMINAL = "nominal"
    DRIFTED = "drifted"
    ANOMALY = "anomaly"


# Transition table: (current_state, condition) -> next_state
_SENSOR_TRANSITIONS = {
    (SensorState.CALIBRATING, "calibration_ok"): SensorState.NOMINAL,
    (SensorState.CALIBRATING, "calibration_timeout"): SensorState.DRIFTED,
    (SensorState.NOMINAL, "drift_detected"): SensorState.DRIFTED,
    (SensorState.NOMINAL, "threshold_exceeded"): SensorState.ANOMALY,
    (SensorState.DRIFTED, "recalibrated"): SensorState.CALIBRATING,
    (SensorState.DRIFTED, "threshold_exceeded"): SensorState.ANOMALY,
    (SensorState.ANOMALY, "recalibrated"): SensorState.CALIBRATING,
    (SensorState.ANOMALY, "cleared"): SensorState.NOMINAL,
}


@dataclass
class Event:
    """Simulation event emitted by an agent decision."""

    entity_id: str
    event_type: str
    tick: int
    data: dict = field(default_factory=dict)


@dataclass
class DecisionResult:
    """Result of an agent decision cycle."""

    action: str
    events: list[Event] = field(default_factory=list)
    llm_called: bool = False


class LLMClient(Protocol):
    """Protocol for LLM API clients (used only by llm-tier agents)."""

    def decide(self, prompt: str, context: dict) -> str: ...


# -- Sensor configuration defaults --

DEFAULT_READING_INTERVAL = 5  # ticks between readings
DEFAULT_CALIBRATION_INTERVAL = 50  # ticks between calibration checks
DEFAULT_DRIFT_THRESHOLD = 0.05  # 5% drift triggers recalibration
DEFAULT_ANOMALY_THRESHOLD = 2.0  # 2x nominal range triggers anomaly


def _evaluate_sensor_condition(entity: Entity, context: dict) -> str:
    """Evaluate which transition condition applies for a sensor entity."""
    state = entity.state.get("sensor_state", SensorState.CALIBRATING)
    reading = context.get("reading")
    calibration = entity.state.get("calibration", {})

    if state == SensorState.CALIBRATING:
        cal_start = entity.state.get("calibration_start_tick", 0)
        current_tick = context.get("tick", 0)
        cal_timeout = entity.config.get("calibration_timeout", 10)
        if current_tick - cal_start >= cal_timeout:
            return "calibration_timeout"
        if calibration.get("complete", False):
            return "calibration_ok"
        return ""

    if state in (SensorState.NOMINAL, SensorState.DRIFTED):
        if reading is not None:
            anomaly_threshold = entity.config.get(
                "anomaly_threshold", DEFAULT_ANOMALY_THRESHOLD
            )
            nominal = calibration.get("nominal_value", 0.0)
            if nominal != 0.0 and abs(reading - nominal) / abs(nominal) > anomaly_threshold:
                return "threshold_exceeded"

        drift = calibration.get("drift", 0.0)
        drift_threshold = entity.config.get(
            "drift_threshold", DEFAULT_DRIFT_THRESHOLD
        )
        if state == SensorState.NOMINAL and abs(drift) > drift_threshold:
            return "drift_detected"

        return ""

    if state == SensorState.ANOMALY:
        if calibration.get("recalibrated", False):
            return "recalibrated"
        if reading is not None:
            nominal = calibration.get("nominal_value", 0.0)
            anomaly_threshold = entity.config.get(
                "anomaly_threshold", DEFAULT_ANOMALY_THRESHOLD
            )
            if nominal == 0.0 or abs(reading - nominal) / abs(nominal) <= anomaly_threshold:
                return "cleared"
        return ""

    return ""


def _decide_sensor(entity: Entity, context: dict) -> DecisionResult:
    """Pure rule-based decision for sensor agents. No LLM calls.

    Deterministic state machine based on:
    - Calibration drift detection
    - Reading schedule adherence
    - Threshold-based anomaly detection

    Returns the same event format as the LLM path for compatibility.
    """
    tick = context.get("tick", 0)
    events: list[Event] = []
    current_state = entity.state.get("sensor_state", SensorState.CALIBRATING)

    # Initialize calibration on first tick
    if "sensor_state" not in entity.state:
        entity.state["sensor_state"] = SensorState.CALIBRATING
        entity.state["calibration_start_tick"] = tick
        entity.state["calibration"] = {"complete": False, "drift": 0.0}
        current_state = SensorState.CALIBRATING

    # Auto-complete calibration after warmup period
    cal = entity.state.get("calibration", {})
    if current_state == SensorState.CALIBRATING and not cal.get("complete", False):
        warmup = entity.config.get("calibration_warmup", 3)
        cal_start = entity.state.get("calibration_start_tick", 0)
        if tick - cal_start >= warmup:
            cal["complete"] = True
            cal["nominal_value"] = entity.config.get("nominal_value", 100.0)
            entity.state["calibration"] = cal

    # Emit scheduled readings
    reading_interval = entity.config.get(
        "reading_interval", DEFAULT_READING_INTERVAL
    )
    if tick % reading_interval == 0 and current_state != SensorState.CALIBRATING:
        reading_value = context.get("reading", cal.get("nominal_value", 0.0))
        events.append(
            Event(
                entity_id=entity.entity_id,
                event_type="sensor_reading",
                tick=tick,
                data={
                    "value": reading_value,
                    "unit": entity.config.get("unit", "units"),
                    "sensor_type": entity.entity_type,
                    "state": current_state.value,
                },
            )
        )

    # Periodic calibration check
    cal_interval = entity.config.get(
        "calibration_interval", DEFAULT_CALIBRATION_INTERVAL
    )
    if tick > 0 and tick % cal_interval == 0 and current_state == SensorState.NOMINAL:
        events.append(
            Event(
                entity_id=entity.entity_id,
                event_type="calibration_check",
                tick=tick,
                data={"drift": cal.get("drift", 0.0), "status": "ok"},
            )
        )

    # Evaluate state transition
    condition = _evaluate_sensor_condition(entity, context)
    if condition:
        next_state_key = (current_state, condition)
        next_state = _SENSOR_TRANSITIONS.get(next_state_key)
        if next_state is not None:
            entity.state["sensor_state"] = next_state

            if next_state == SensorState.ANOMALY:
                events.append(
                    Event(
                        entity_id=entity.entity_id,
                        event_type="anomaly_detected",
                        tick=tick,
                        data={
                            "previous_state": current_state.value,
                            "reading": context.get("reading"),
                            "threshold": entity.config.get(
                                "anomaly_threshold", DEFAULT_ANOMALY_THRESHOLD
                            ),
                        },
                    )
                )
            elif next_state == SensorState.DRIFTED:
                events.append(
                    Event(
                        entity_id=entity.entity_id,
                        event_type="calibration_drift",
                        tick=tick,
                        data={
                            "drift": cal.get("drift", 0.0),
                            "threshold": entity.config.get(
                                "drift_threshold", DEFAULT_DRIFT_THRESHOLD
                            ),
                        },
                    )
                )
            elif condition == "recalibrated":
                entity.state["calibration_start_tick"] = tick
                entity.state["calibration"] = {
                    "complete": False,
                    "drift": 0.0,
                    "recalibrated": False,
                }

    action = f"sensor:{current_state.value}"
    return DecisionResult(action=action, events=events, llm_called=False)


def _decide_llm(
    entity: Entity, context: dict, client: LLMClient | None = None
) -> DecisionResult:
    """LLM-based decision for agents that need reasoning.

    In dry-run mode (client=None), returns a synthetic decision.
    In live mode, calls the LLM API via the provided client.
    """
    tick = context.get("tick", 0)

    if client is None:
        # Dry-run: synthetic decision without API call
        return DecisionResult(
            action=f"dry_run:{entity.entity_type}:idle",
            events=[
                Event(
                    entity_id=entity.entity_id,
                    event_type="agent_decision",
                    tick=tick,
                    data={
                        "mode": "dry_run",
                        "entity_type": entity.entity_type,
                        "decision": "idle",
                    },
                )
            ],
            llm_called=False,
        )

    # Live mode: call LLM
    prompt = (
        f"Entity {entity.entity_id} ({entity.entity_type}) at tick {tick}. "
        f"State: {entity.state}. Context: {context}. "
        f"What action should this agent take?"
    )
    response = client.decide(prompt, context)

    return DecisionResult(
        action=f"llm:{entity.entity_type}:{response}",
        events=[
            Event(
                entity_id=entity.entity_id,
                event_type="agent_decision",
                tick=tick,
                data={
                    "mode": "llm",
                    "entity_type": entity.entity_type,
                    "decision": response,
                },
            )
        ],
        llm_called=True,
    )


def decide(
    entity: Entity, context: dict, client: LLMClient | None = None
) -> DecisionResult:
    """Route decision to appropriate handler based on entity's llm_tier.

    - rule_based: Deterministic sensor state machine (no LLM)
    - llm: LLM reasoning (or dry-run if no client provided)
    """
    if entity.llm_tier == "rule_based":
        return _decide_sensor(entity, context)
    return _decide_llm(entity, context, client)
