"""Tests for the LLM decision module.

Validates that:
- Sensor agents use rule-based logic with zero LLM calls
- State machine transitions are deterministic
- Event output matches expected format
- LLM-tier agents produce dry-run events when no client provided
"""

import pytest

from ..llm import (
    DecisionResult,
    Event,
    SensorState,
    _decide_sensor,
    _decide_llm,
    decide,
)
from ..orchestrator import Entity


def _make_sensor(entity_id="sensor-01", entity_type="load_cell", config=None):
    """Create a sensor entity for testing."""
    return Entity(
        entity_id=entity_id,
        entity_type=entity_type,
        llm_tier="rule_based",
        config=config or {"nominal_value": 100.0, "calibration_warmup": 3},
    )


def _make_llm_entity(entity_id="equip-01", entity_type="crane"):
    """Create an LLM-tier entity for testing."""
    return Entity(
        entity_id=entity_id,
        entity_type=entity_type,
        llm_tier="llm",
    )


class TestDecideSensor:
    """Tests for _decide_sensor (rule-based path)."""

    def test_no_llm_calls(self):
        """Sensor decisions must never set llm_called=True."""
        entity = _make_sensor()
        for tick in range(100):
            result = _decide_sensor(entity, {"tick": tick})
            assert result.llm_called is False

    def test_initial_state_is_calibrating(self):
        """New sensor starts in CALIBRATING state."""
        entity = _make_sensor()
        _decide_sensor(entity, {"tick": 0})
        assert entity.state["sensor_state"] == SensorState.CALIBRATING

    def test_transitions_to_nominal_after_warmup(self):
        """Sensor transitions to NOMINAL after calibration warmup."""
        entity = _make_sensor(config={"nominal_value": 100.0, "calibration_warmup": 3})
        # Run through calibration
        for tick in range(5):
            _decide_sensor(entity, {"tick": tick})
        assert entity.state["sensor_state"] == SensorState.NOMINAL

    def test_emits_readings_at_interval(self):
        """Sensor emits reading events at configured interval."""
        entity = _make_sensor(
            config={
                "nominal_value": 100.0,
                "calibration_warmup": 1,
                "reading_interval": 5,
            }
        )
        # Get past calibration
        for tick in range(3):
            _decide_sensor(entity, {"tick": tick})

        # Check reading emission at interval boundaries
        result = _decide_sensor(entity, {"tick": 5, "reading": 99.5})
        reading_events = [e for e in result.events if e.event_type == "sensor_reading"]
        assert len(reading_events) == 1
        assert reading_events[0].data["value"] == 99.5

    def test_anomaly_detection(self):
        """Sensor transitions to ANOMALY when threshold exceeded."""
        entity = _make_sensor(
            config={
                "nominal_value": 100.0,
                "calibration_warmup": 1,
                "anomaly_threshold": 0.5,
            }
        )
        # Get to nominal
        for tick in range(3):
            _decide_sensor(entity, {"tick": tick})
        assert entity.state["sensor_state"] == SensorState.NOMINAL

        # Provide extreme reading (>50% off nominal)
        result = _decide_sensor(entity, {"tick": 5, "reading": 200.0})
        assert entity.state["sensor_state"] == SensorState.ANOMALY
        anomaly_events = [e for e in result.events if e.event_type == "anomaly_detected"]
        assert len(anomaly_events) == 1

    def test_drift_detection(self):
        """Sensor transitions to DRIFTED when calibration drift exceeds threshold."""
        entity = _make_sensor(
            config={
                "nominal_value": 100.0,
                "calibration_warmup": 1,
                "drift_threshold": 0.05,
            }
        )
        # Get to nominal
        for tick in range(3):
            _decide_sensor(entity, {"tick": tick})
        assert entity.state["sensor_state"] == SensorState.NOMINAL

        # Simulate drift
        entity.state["calibration"]["drift"] = 0.1
        result = _decide_sensor(entity, {"tick": 5})
        assert entity.state["sensor_state"] == SensorState.DRIFTED
        drift_events = [e for e in result.events if e.event_type == "calibration_drift"]
        assert len(drift_events) == 1

    def test_recalibration_from_anomaly(self):
        """Sensor can return to CALIBRATING after anomaly via recalibration."""
        entity = _make_sensor(
            config={
                "nominal_value": 100.0,
                "calibration_warmup": 1,
                "anomaly_threshold": 0.5,
            }
        )
        # Get to nominal then anomaly
        for tick in range(3):
            _decide_sensor(entity, {"tick": tick})
        _decide_sensor(entity, {"tick": 5, "reading": 200.0})
        assert entity.state["sensor_state"] == SensorState.ANOMALY

        # Trigger recalibration
        entity.state["calibration"]["recalibrated"] = True
        _decide_sensor(entity, {"tick": 6})
        assert entity.state["sensor_state"] == SensorState.CALIBRATING

    def test_deterministic_across_runs(self):
        """Same inputs produce identical outputs (deterministic)."""
        results_a = []
        results_b = []

        for run_results in (results_a, results_b):
            entity = _make_sensor(config={"nominal_value": 100.0, "calibration_warmup": 2})
            for tick in range(20):
                result = _decide_sensor(entity, {"tick": tick, "reading": 100.0 + tick * 0.01})
                run_results.append((result.action, len(result.events)))

        assert results_a == results_b

    def test_event_format_matches_llm_path(self):
        """Sensor events use same Event dataclass as LLM events."""
        entity = _make_sensor(
            config={"nominal_value": 100.0, "calibration_warmup": 1, "reading_interval": 1}
        )
        for tick in range(3):
            _decide_sensor(entity, {"tick": tick})

        result = _decide_sensor(entity, {"tick": 5, "reading": 100.0})
        for event in result.events:
            assert isinstance(event, Event)
            assert isinstance(event.entity_id, str)
            assert isinstance(event.event_type, str)
            assert isinstance(event.tick, int)
            assert isinstance(event.data, dict)


class TestDecideLLM:
    """Tests for _decide_llm (LLM path)."""

    def test_dry_run_no_client(self):
        """Without client, produces dry-run events with no LLM call."""
        entity = _make_llm_entity()
        result = _decide_llm(entity, {"tick": 0}, client=None)
        assert result.llm_called is False
        assert "dry_run" in result.action
        assert result.events[0].data["mode"] == "dry_run"

    def test_live_mode_calls_client(self):
        """With client, makes an LLM call and records it."""

        class MockClient:
            def __init__(self):
                self.called = False

            def decide(self, prompt, context):
                self.called = True
                return "move_container"

        client = MockClient()
        entity = _make_llm_entity()
        result = _decide_llm(entity, {"tick": 0}, client=client)
        assert client.called
        assert result.llm_called is True
        assert "move_container" in result.action


class TestDecideRouter:
    """Tests for the decide() routing function."""

    def test_routes_sensor_to_rule_based(self):
        """rule_based entities go through _decide_sensor."""
        entity = _make_sensor()
        result = decide(entity, {"tick": 0})
        assert result.llm_called is False
        assert "sensor:" in result.action

    def test_routes_llm_entity_to_llm(self):
        """llm entities go through _decide_llm."""
        entity = _make_llm_entity()
        result = decide(entity, {"tick": 0})
        assert "dry_run" in result.action

    def test_sensor_never_calls_llm_with_client(self):
        """Even with an LLM client available, sensors bypass it."""

        class MockClient:
            def __init__(self):
                self.called = False

            def decide(self, prompt, context):
                self.called = True
                return "noop"

        client = MockClient()
        entity = _make_sensor()
        for tick in range(50):
            decide(entity, {"tick": tick}, client=client)
        assert not client.called
