"""Tests for labor constraint modeling (hi-84h5).

ILA Local 1414 union rules: shift limits, mandatory breaks, crew minimums.
"""

import asyncio
import json

from port_agent.llm import DryRunProvider
from port_agent.orchestrator import (
    AgentSpec,
    OrchestratorConfig,
    Orchestrator,
    parse_agent_composition,
)
from port_agent_bridge.bridge_api import BridgeAPI, MINIMUM_CREW_PER_CRANE
from port_agent_bridge.lifecycle import (
    LaborConstraintTracker,
    LaborState,
    LifecycleManager,
    _ROLE_CONFIGS,
    MINIMUM_CREW_PER_CRANE as LIFECYCLE_MIN_CREW,
)
from port_agent_bridge.hive_state import HiveStateStore


# ---------------------------------------------------------------------------
#  LaborConstraintTracker unit tests
# ---------------------------------------------------------------------------

class TestLaborConstraintTracker:
    def test_initial_state(self):
        lct = LaborConstraintTracker("op-1")
        assert lct.consecutive_work_hours == 0.0
        assert lct.remaining_shift_hours == 12.0
        assert not lct.break_eligible
        assert not lct.break_required
        assert not lct.shift_expired
        assert not lct.state.on_break
        assert not lct.state.shift_ended

    def test_work_accumulates(self):
        lct = LaborConstraintTracker("op-1", max_consecutive_hours=6.0)
        lct.tick(sim_minutes=60.0)  # 1 hour
        assert abs(lct.consecutive_work_hours - 1.0) < 0.01

    def test_break_eligible_at_80_pct(self):
        lct = LaborConstraintTracker("op-1", max_consecutive_hours=6.0)
        # 80% of 6 hours = 4.8 hours = 288 minutes
        lct.tick(sim_minutes=290.0)
        assert lct.break_eligible
        assert not lct.break_required

    def test_break_required_at_max(self):
        lct = LaborConstraintTracker("op-1", max_consecutive_hours=6.0)
        # 6 hours = 360 minutes
        lct.tick(sim_minutes=360.0)
        assert lct.break_required

    def test_mandatory_break_event(self):
        lct = LaborConstraintTracker("op-1", max_consecutive_hours=6.0)
        events = lct.tick(sim_minutes=361.0)
        mandatory = [e for e in events if e["event_type"] == "MANDATORY_BREAK_REQUIRED"]
        assert len(mandatory) == 1
        assert mandatory[0]["priority"] == "HIGH"
        assert mandatory[0]["details"]["worker_id"] == "op-1"

    def test_break_approaching_event(self):
        lct = LaborConstraintTracker("op-1", max_consecutive_hours=6.0)
        events = lct.tick(sim_minutes=300.0)  # 5 hours > 80%
        approaching = [e for e in events if e["event_type"] == "BREAK_APPROACHING"]
        assert len(approaching) == 1

    def test_start_break(self):
        lct = LaborConstraintTracker("op-1")
        lct.tick(sim_minutes=100.0)
        events = lct.start_break(sim_minutes=100.0)
        assert len(events) == 1
        assert events[0]["event_type"] == "BREAK_STARTED"
        assert lct.state.on_break
        assert lct.state.breaks_taken == 1

    def test_break_completes(self):
        lct = LaborConstraintTracker("op-1", break_duration_min=30.0)
        lct.tick(sim_minutes=100.0)
        lct.start_break(sim_minutes=100.0)
        # Tick during break — should complete after 30 min
        events = lct.tick(sim_minutes=131.0)
        completed = [e for e in events if e["event_type"] == "BREAK_COMPLETED"]
        assert len(completed) == 1
        assert not lct.state.on_break
        # Consecutive work resets after break
        assert lct.state.consecutive_work_minutes == 0.0

    def test_shift_expires(self):
        lct = LaborConstraintTracker("op-1", shift_duration_hours=12.0)
        # Work 12 hours = 720 minutes
        lct.tick(sim_minutes=720.0)
        assert lct.shift_expired
        assert lct.state.shift_ended

    def test_shift_ended_event(self):
        lct = LaborConstraintTracker("op-1", shift_duration_hours=12.0)
        events = lct.tick(sim_minutes=721.0)
        ended = [e for e in events if e["event_type"] == "SHIFT_ENDED"]
        assert len(ended) == 1
        assert ended[0]["priority"] == "HIGH"

    def test_no_events_after_shift_ended(self):
        lct = LaborConstraintTracker("op-1", shift_duration_hours=1.0)
        lct.tick(sim_minutes=61.0)
        assert lct.state.shift_ended
        events = lct.tick(sim_minutes=10.0)
        assert len(events) == 0

    def test_no_break_after_shift_ended(self):
        lct = LaborConstraintTracker("op-1", shift_duration_hours=1.0)
        lct.tick(sim_minutes=61.0)
        events = lct.start_break(sim_minutes=62.0)
        assert len(events) == 0

    def test_remaining_shift_accounts_for_breaks(self):
        lct = LaborConstraintTracker("op-1", shift_duration_hours=12.0, break_duration_min=30.0)
        # Work 5 hours
        lct.tick(sim_minutes=300.0)
        # Take a break (30 min)
        lct.start_break(sim_minutes=300.0)
        lct.tick(sim_minutes=331.0)  # break completes
        # Shift elapsed = 5h work + 0.5h break = 5.5h; remaining = 6.5h
        assert abs(lct.remaining_shift_hours - 6.5) < 0.1

    def test_custom_shift_start(self):
        lct = LaborConstraintTracker("op-1", shift_start_minutes=60.0)
        assert lct.state.shift_start_minutes == 60.0


# ---------------------------------------------------------------------------
#  LaborState dataclass
# ---------------------------------------------------------------------------

class TestLaborState:
    def test_defaults(self):
        state = LaborState()
        assert state.shift_start_minutes == 0.0
        assert state.consecutive_work_minutes == 0.0
        assert not state.on_break
        assert state.breaks_taken == 0
        assert not state.shift_ended


# ---------------------------------------------------------------------------
#  _ROLE_CONFIGS labor entries
# ---------------------------------------------------------------------------

class TestRoleConfigsLabor:
    def test_crane_has_labor_config(self):
        cfg = _ROLE_CONFIGS["crane"]
        assert "labor" in cfg
        labor = cfg["labor"]
        assert labor["max_consecutive_hours"] == 6.0
        assert labor["break_duration_min"] == 30.0
        assert labor["shift_duration_hours"] == 12.0
        assert labor["hazmat_cert_required"] is True
        assert labor["minimum_crew"] == LIFECYCLE_MIN_CREW

    def test_operator_has_labor_config(self):
        cfg = _ROLE_CONFIGS["operator"]
        assert "labor" in cfg
        assert cfg["labor"]["max_consecutive_hours"] == 6.0

    def test_tractor_has_labor_config(self):
        cfg = _ROLE_CONFIGS["tractor"]
        assert "labor" in cfg
        assert cfg["labor"]["shift_duration_hours"] == 12.0

    def test_sensor_no_labor_config(self):
        cfg = _ROLE_CONFIGS["sensor"]
        assert "labor" not in cfg

    def test_virtual_roles_no_labor(self):
        for role in ("berth_manager", "yard_block"):
            cfg = _ROLE_CONFIGS[role]
            assert "labor" not in cfg

    def test_minimum_crew_constant(self):
        assert MINIMUM_CREW_PER_CRANE == 2
        assert LIFECYCLE_MIN_CREW == 2


# ---------------------------------------------------------------------------
#  LifecycleManager integration with labor
# ---------------------------------------------------------------------------

class TestLifecycleManagerLabor:
    def test_crane_has_labor_tracker(self):
        mgr = LifecycleManager("crane-1", role="crane")
        assert mgr.labor is not None

    def test_operator_has_labor_tracker(self):
        mgr = LifecycleManager("op-1", role="operator")
        assert mgr.labor is not None

    def test_tractor_has_labor_tracker(self):
        mgr = LifecycleManager("tractor-1", role="tractor")
        assert mgr.labor is not None

    def test_sensor_no_labor_tracker(self):
        mgr = LifecycleManager("sensor-1", role="sensor")
        assert mgr.labor is None

    def test_virtual_role_no_labor_tracker(self):
        mgr = LifecycleManager("berth-1", role="berth_manager")
        assert mgr.labor is None

    def test_tick_emits_labor_events(self):
        mgr = LifecycleManager("op-1", role="operator")
        # Accumulate enough time for break approaching
        all_events = []
        for i in range(1, 201):  # 200 ticks × 1.5 min = 300 min = 5 hours
            evts = mgr.tick(cycle=i, action="wait", node_id="op-1", sim_minutes=i * 1.5)
            all_events.extend(evts)
        approaching = [e for e in all_events if e["event_type"] == "BREAK_APPROACHING"]
        assert len(approaching) > 0

    def test_shift_start_param(self):
        mgr = LifecycleManager("op-1", role="operator", shift_start_minutes=60.0)
        assert mgr.labor.state.shift_start_minutes == 60.0


# ---------------------------------------------------------------------------
#  DryRunProvider labor constraint decisions
# ---------------------------------------------------------------------------

class TestDryRunProviderLabor:
    def _run(self, coro):
        return asyncio.get_event_loop().run_until_complete(coro)

    def test_shift_ended_goes_off_duty(self):
        provider = DryRunProvider(role="operator", proficiency="expert")
        decision = self._run(provider.decide(
            persona="test",
            observed_state={
                "labor_constraints": {"shift_ended": True, "shift_elapsed_hours": 12.1},
                "tasking": {},
            },
            available_tools=[],
        ))
        assert decision.action == "report_available"
        assert decision.arguments["status"] == "OFF_SHIFT"

    def test_break_required_takes_break(self):
        provider = DryRunProvider(role="operator", proficiency="expert")
        decision = self._run(provider.decide(
            persona="test",
            observed_state={
                "labor_constraints": {
                    "break_required": True,
                    "consecutive_hours": 6.1,
                    "max_consecutive_hours": 6.0,
                },
                "tasking": {},
            },
            available_tools=[],
        ))
        assert decision.action == "report_available"
        assert decision.arguments["status"] == "BREAK"

    def test_on_break_waits(self):
        provider = DryRunProvider(role="crane", proficiency="expert")
        decision = self._run(provider.decide(
            persona="test",
            observed_state={
                "labor_constraints": {"on_break": True, "break_duration_min": 30},
                "container_queue": {"next_containers": []},
            },
            available_tools=[],
        ))
        assert decision.action == "wait"

    def test_crew_insufficient_requests_support(self):
        provider = DryRunProvider(role="crane", proficiency="expert")
        decision = self._run(provider.decide(
            persona="test",
            observed_state={
                "labor_constraints": {
                    "crew_insufficient": True,
                    "current_crew": 1,
                    "minimum_crew": 2,
                },
                "container_queue": {"next_containers": [{"container_id": "C1"}]},
            },
            available_tools=[],
        ))
        assert decision.action == "request_support"
        assert "SIGNALER" in decision.arguments["capability_needed"]

    def test_no_labor_constraints_normal_operation(self):
        provider = DryRunProvider(role="crane", proficiency="expert")
        decision = self._run(provider.decide(
            persona="test",
            observed_state={
                "container_queue": {"next_containers": [{"container_id": "C1", "status": "QUEUED"}]},
            },
            available_tools=[],
        ))
        # Without labor constraints, should process normally
        assert decision.action in ("complete_container_move", "request_support", "wait")

    def test_tractor_shift_ended_waits(self):
        provider = DryRunProvider(role="tractor", proficiency="expert")
        decision = self._run(provider.decide(
            persona="test",
            observed_state={
                "labor_constraints": {"shift_ended": True, "shift_elapsed_hours": 12.5},
                "tasking": {},
            },
            available_tools=[],
        ))
        assert decision.action == "wait"
        assert "Shift ended" in decision.arguments["reason"]


# ---------------------------------------------------------------------------
#  BridgeAPI labor constraint tasking
# ---------------------------------------------------------------------------

class TestBridgeAPILaborTasking:
    def _make_bridge(self, role="crane", node_id="crane-1"):
        store = HiveStateStore()
        # Create minimal entity doc with labor state
        store.create_document(
            collection="node_states",
            doc_id=f"sim_doc_{node_id}",
            fields={
                "node_id": node_id,
                "entity_type": role,
                "labor": {
                    "shift_elapsed_hours": 4.0,
                    "consecutive_hours": 2.5,
                    "max_consecutive_hours": 6.0,
                    "break_duration_min": 30.0,
                    "shift_duration_hours": 12.0,
                    "remaining_shift_hours": 8.0,
                    "on_break": False,
                    "break_eligible": False,
                    "break_required": False,
                    "shift_ended": False,
                    "breaks_taken": 0,
                },
                "assignment": {},
                "operational_status": "OPERATIONAL",
            },
        )
        # Create team/queue docs for crane
        if role == "crane":
            store.create_document(
                collection="team_summaries",
                doc_id="team_hold-3",
                fields={"team_members": {}, "hold_id": "hold-3"},
            )
            store.create_document(
                collection="container_queues",
                doc_id="queue_hold-3",
                fields={"total_containers": 10, "completed_count": 0, "containers": []},
            )
        return BridgeAPI(store=store, node_id=node_id, role=role, hold_id="hold-3"), store

    def test_crane_tasking_includes_labor(self):
        bridge, _ = self._make_bridge(role="crane")
        result = asyncio.get_event_loop().run_until_complete(
            bridge.read_resource("hive://tasking")
        )
        tasking = json.loads(result.contents[0].text)
        assert "labor_constraints" in tasking
        lc = tasking["labor_constraints"]
        assert lc["remaining_shift_hours"] == 8.0
        assert lc["consecutive_hours"] == 2.5

    def test_crane_tasking_includes_crew_info(self):
        bridge, _ = self._make_bridge(role="crane")
        result = asyncio.get_event_loop().run_until_complete(
            bridge.read_resource("hive://tasking")
        )
        tasking = json.loads(result.contents[0].text)
        lc = tasking["labor_constraints"]
        assert "crew_insufficient" in lc
        assert "minimum_crew" in lc
        assert lc["minimum_crew"] == MINIMUM_CREW_PER_CRANE

    def test_operator_tasking_includes_labor(self):
        bridge, _ = self._make_bridge(role="operator", node_id="op-1")
        result = asyncio.get_event_loop().run_until_complete(
            bridge.read_resource("hive://tasking")
        )
        tasking = json.loads(result.contents[0].text)
        assert "labor_constraints" in tasking

    def test_labor_violation_event_shift_approaching(self):
        store = HiveStateStore()
        store.create_document(
            collection="node_states",
            doc_id="sim_doc_op-1",
            fields={
                "node_id": "op-1",
                "entity_type": "operator",
                "labor": {
                    "shift_elapsed_hours": 11.5,
                    "consecutive_hours": 2.0,
                    "max_consecutive_hours": 6.0,
                    "break_duration_min": 30.0,
                    "shift_duration_hours": 12.0,
                    "remaining_shift_hours": 0.5,
                    "on_break": False,
                    "break_eligible": False,
                    "break_required": False,
                    "shift_ended": False,
                    "breaks_taken": 1,
                },
                "assignment": {},
                "operational_status": "AVAILABLE",
            },
        )
        bridge = BridgeAPI(store=store, node_id="op-1", role="operator", hold_id="hold-3")
        asyncio.get_event_loop().run_until_complete(
            bridge.read_resource("hive://tasking")
        )
        events = store.get_events()
        violations = [e for e in events if e["event_type"] == "labor_constraint_violation"]
        assert len(violations) >= 1
        assert violations[0]["details"]["constraint"] == "shift_limit_approaching"

    def test_labor_violation_event_break_overdue(self):
        store = HiveStateStore()
        store.create_document(
            collection="node_states",
            doc_id="sim_doc_op-1",
            fields={
                "node_id": "op-1",
                "entity_type": "operator",
                "labor": {
                    "shift_elapsed_hours": 7.0,
                    "consecutive_hours": 7.0,
                    "max_consecutive_hours": 6.0,
                    "break_duration_min": 30.0,
                    "shift_duration_hours": 12.0,
                    "remaining_shift_hours": 5.0,
                    "on_break": False,
                    "break_eligible": True,
                    "break_required": False,
                    "shift_ended": False,
                    "breaks_taken": 0,
                },
                "assignment": {},
                "operational_status": "AVAILABLE",
            },
        )
        bridge = BridgeAPI(store=store, node_id="op-1", role="operator", hold_id="hold-3")
        asyncio.get_event_loop().run_until_complete(
            bridge.read_resource("hive://tasking")
        )
        events = store.get_events()
        violations = [e for e in events if e["event_type"] == "labor_constraint_violation"]
        assert len(violations) >= 1
        assert violations[0]["details"]["constraint"] == "mandatory_break_overdue"


# ---------------------------------------------------------------------------
#  Orchestrator labor initialization
# ---------------------------------------------------------------------------

class TestOrchestratorLaborInit:
    def test_crane_entity_has_labor_fields(self):
        config = OrchestratorConfig(
            agents=parse_agent_composition("1c", provider="dry-run"),
        )
        orch = Orchestrator(config)
        orch.initialize_state()
        entity = orch.store.get_document("node_states", "sim_doc_crane-1")
        assert entity is not None
        labor = entity.fields.get("labor")
        assert labor is not None
        assert labor["shift_start_minutes"] == 0.0
        assert labor["max_consecutive_hours"] == 6.0
        assert labor["shift_duration_hours"] == 12.0
        assert labor["remaining_shift_hours"] == 12.0
        assert labor["breaks_taken"] == 0
        assert labor["hazmat_cert_required"] is True
        assert labor["minimum_crew"] == 2

    def test_operator_entity_has_labor_fields(self):
        config = OrchestratorConfig(
            agents=parse_agent_composition("1w", provider="dry-run"),
        )
        orch = Orchestrator(config)
        orch.initialize_state()
        entity = orch.store.get_document("node_states", "sim_doc_op-1")
        assert entity is not None
        labor = entity.fields.get("labor")
        assert labor is not None
        assert labor["shift_start_minutes"] == 0.0
        assert labor["consecutive_hours"] == 0.0
        assert not labor["shift_ended"]

    def test_tractor_entity_has_labor_fields(self):
        config = OrchestratorConfig(
            agents=parse_agent_composition("1t", provider="dry-run"),
        )
        orch = Orchestrator(config)
        orch.initialize_state()
        entity = orch.store.get_document("node_states", "sim_doc_tractor-1")
        assert entity is not None
        labor = entity.fields.get("labor")
        assert labor is not None
        assert labor["break_duration_min"] == 30.0

    def test_sensor_entity_no_labor_fields(self):
        config = OrchestratorConfig(
            agents=parse_agent_composition("1x", provider="dry-run"),
        )
        orch = Orchestrator(config)
        orch.initialize_state()
        entity = orch.store.get_document("node_states", "sim_doc_load-cell-1")
        assert entity is not None
        labor = entity.fields.get("labor")
        assert labor is None

    def test_full_composition_all_workers_have_labor(self):
        config = OrchestratorConfig(
            agents=parse_agent_composition("2c5w4t1s1a2x", provider="dry-run"),
        )
        orch = Orchestrator(config)
        orch.initialize_state()
        # All cranes, operators, tractors should have labor
        for spec in config.agents:
            entity = orch.store.get_document("node_states", f"sim_doc_{spec.node_id}")
            if entity and spec.role in ("crane", "operator", "tractor"):
                assert entity.fields.get("labor") is not None, f"{spec.node_id} missing labor"
