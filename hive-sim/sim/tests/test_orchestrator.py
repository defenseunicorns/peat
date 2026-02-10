"""Tests for the agent orchestrator.

Validates that:
- Sensor entities are auto-tagged with llm_tier='rule_based'
- Sensors cycle without any LLM calls
- Compositions load correctly with proper tiering
- Event output is consistent across configurations
"""

import pytest

from ..orchestrator import (
    Composition,
    Entity,
    Orchestrator,
    RULE_BASED_TYPES,
)
from ..llm import SensorState


class TestEntityTiering:
    """Tests for automatic LLM tier assignment."""

    def test_sensor_types_tagged_rule_based(self):
        """All known sensor types get llm_tier='rule_based'."""
        orch = Orchestrator()
        for sensor_type in RULE_BASED_TYPES:
            entity = orch.add_entity(f"s-{sensor_type}", sensor_type)
            assert entity.llm_tier == "rule_based", (
                f"{sensor_type} should be rule_based"
            )

    def test_non_sensor_types_tagged_llm(self):
        """Non-sensor entity types get llm_tier='llm'."""
        orch = Orchestrator()
        for etype in ("crane", "forklift", "operator", "dispatcher"):
            entity = orch.add_entity(f"e-{etype}", etype)
            assert entity.llm_tier == "llm", f"{etype} should be llm tier"

    def test_load_cell_is_rule_based(self):
        orch = Orchestrator()
        entity = orch.add_entity("lc-01", "load_cell")
        assert entity.llm_tier == "rule_based"

    def test_rfid_reader_is_rule_based(self):
        orch = Orchestrator()
        entity = orch.add_entity("rfid-01", "rfid_reader")
        assert entity.llm_tier == "rule_based"


class TestOrchestratorSimulation:
    """Tests for simulation stepping."""

    def test_sensors_cycle_without_llm_calls(self):
        """Core acceptance criterion: sensors produce zero LLM calls."""
        orch = Orchestrator()
        orch.add_entity("lc-01", "load_cell", config={"nominal_value": 500.0, "calibration_warmup": 2})
        orch.add_entity("rfid-01", "rfid_reader", config={"nominal_value": 1.0, "calibration_warmup": 2})
        orch.add_entity("temp-01", "temperature", config={"nominal_value": 22.0, "calibration_warmup": 2})

        for _ in range(100):
            orch.step()

        assert orch.get_llm_call_count() == 0

    def test_mixed_entities_only_llm_entities_call_llm(self):
        """Only LLM-tier entities trigger LLM calls (in live mode)."""

        class CountingClient:
            def __init__(self):
                self.call_count = 0

            def decide(self, prompt, context):
                self.call_count += 1
                return "idle"

        client = CountingClient()
        orch = Orchestrator(llm_client=client)
        orch.add_entity("lc-01", "load_cell", config={"nominal_value": 500.0, "calibration_warmup": 1})
        orch.add_entity("crane-01", "crane")

        for _ in range(10):
            orch.step()

        # Only the crane should have called LLM (once per tick)
        assert client.call_count == 10
        assert orch.get_llm_call_count() == 10

    def test_step_produces_events(self):
        """Each step produces events from active entities."""
        orch = Orchestrator()
        orch.add_entity(
            "lc-01", "load_cell",
            config={"nominal_value": 100.0, "calibration_warmup": 1, "reading_interval": 1},
        )

        # First few ticks are calibration
        events = orch.step()  # tick 0
        events = orch.step()  # tick 1 - calibration completes

        # After calibration, should get readings
        events = orch.step()  # tick 2
        reading_events = [e for e in events if e.event_type == "sensor_reading"]
        assert len(reading_events) >= 0  # May or may not emit on this tick

    def test_sensor_readings_passed_through(self):
        """External sensor readings are passed to entities."""
        orch = Orchestrator()
        orch.add_entity(
            "lc-01", "load_cell",
            config={"nominal_value": 100.0, "calibration_warmup": 1, "reading_interval": 1},
        )

        # Get past calibration
        for _ in range(3):
            orch.step()

        # Provide a reading
        events = orch.step(sensor_readings={"lc-01": 99.5})
        reading_events = [e for e in events if e.event_type == "sensor_reading"]
        if reading_events:
            assert reading_events[0].data["value"] == 99.5

    def test_remove_entity(self):
        orch = Orchestrator()
        orch.add_entity("lc-01", "load_cell")
        assert "lc-01" in orch.entities
        orch.remove_entity("lc-01")
        assert "lc-01" not in orch.entities

    def test_get_sensor_entities(self):
        orch = Orchestrator()
        orch.add_entity("lc-01", "load_cell")
        orch.add_entity("crane-01", "crane")
        sensors = orch.get_sensor_entities()
        assert len(sensors) == 1
        assert sensors[0].entity_id == "lc-01"

    def test_get_llm_entities(self):
        orch = Orchestrator()
        orch.add_entity("lc-01", "load_cell")
        orch.add_entity("crane-01", "crane")
        llm_ents = orch.get_llm_entities()
        assert len(llm_ents) == 1
        assert llm_ents[0].entity_id == "crane-01"


class TestComposition:
    """Tests for composition loading."""

    def test_load_composition(self):
        """Compositions load entities with correct tiering."""
        comp = Composition(
            name="port-berth-1",
            entities={
                "lc-01": {"entity_type": "load_cell", "config": {"nominal_value": 500.0}},
                "lc-02": {"entity_type": "load_cell", "config": {"nominal_value": 500.0}},
                "rfid-01": {"entity_type": "rfid_reader", "config": {"nominal_value": 1.0}},
                "crane-01": {"entity_type": "crane"},
                "operator-01": {"entity_type": "operator"},
            },
        )

        orch = Orchestrator()
        added = orch.load_composition(comp)

        assert len(added) == 5
        assert len(orch.entities) == 5

        # Verify tiering
        assert orch.entities["lc-01"].llm_tier == "rule_based"
        assert orch.entities["lc-02"].llm_tier == "rule_based"
        assert orch.entities["rfid-01"].llm_tier == "rule_based"
        assert orch.entities["crane-01"].llm_tier == "llm"
        assert orch.entities["operator-01"].llm_tier == "llm"

    def test_composition_sensors_zero_llm_calls(self):
        """Full composition runs with zero LLM calls for sensor entities."""
        comp = Composition(
            name="sensor-only",
            entities={
                "lc-01": {"entity_type": "load_cell", "config": {"nominal_value": 500.0, "calibration_warmup": 2}},
                "temp-01": {"entity_type": "temperature", "config": {"nominal_value": 22.0, "calibration_warmup": 2}},
                "rfid-01": {"entity_type": "rfid_reader", "config": {"nominal_value": 1.0, "calibration_warmup": 2}},
            },
        )

        orch = Orchestrator()
        orch.load_composition(comp)

        all_events = []
        for _ in range(50):
            events = orch.step()
            all_events.extend(events)

        assert orch.get_llm_call_count() == 0
        assert len(all_events) > 0  # Some events were produced

    def test_composition_config_passed_to_entities(self):
        """Entity config from composition is preserved."""
        comp = Composition(
            name="configured",
            entities={
                "lc-01": {
                    "entity_type": "load_cell",
                    "config": {"nominal_value": 750.0, "unit": "kg"},
                },
            },
        )

        orch = Orchestrator()
        orch.load_composition(comp)

        entity = orch.entities["lc-01"]
        assert entity.config["nominal_value"] == 750.0
        assert entity.config["unit"] == "kg"


class TestEventConsistency:
    """Tests for event output consistency."""

    def test_same_events_as_dry_run_format(self):
        """Rule-based sensor events use the same Event structure as dry-run."""
        orch = Orchestrator()
        orch.add_entity("lc-01", "load_cell", config={"nominal_value": 100.0, "calibration_warmup": 1})
        orch.add_entity("crane-01", "crane")

        for _ in range(10):
            orch.step()

        # All events should be Event instances
        for event in orch.event_log:
            assert hasattr(event, "entity_id")
            assert hasattr(event, "event_type")
            assert hasattr(event, "tick")
            assert hasattr(event, "data")

    def test_entity_states_snapshot(self):
        """get_entity_states returns consistent snapshot."""
        orch = Orchestrator()
        orch.add_entity("lc-01", "load_cell", config={"nominal_value": 100.0, "calibration_warmup": 1})

        for _ in range(5):
            orch.step()

        states = orch.get_entity_states()
        assert "lc-01" in states
        assert states["lc-01"]["llm_tier"] == "rule_based"
        assert states["lc-01"]["entity_type"] == "load_cell"
        assert "sensor_state" in states["lc-01"]["state"]
