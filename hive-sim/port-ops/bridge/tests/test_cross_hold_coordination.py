"""Tests for cross-hold coordination and rebalancing (hi-2iui).

Validates:
- New berth manager tools: reassign_worker, escalate_to_scheduler, update_hold_priority
- Enhanced berth manager tasking includes equipment status and worker counts
- DryRun berth manager detects imbalance causes and takes corrective action
- New events: worker_reassigned, hold_priority_changed, scheduler_escalation
"""

import asyncio
import json
import unittest

from port_agent_bridge.hive_state import (
    HiveStateStore,
    create_container_queue,
    create_sample_containers,
    create_team_state,
    create_transport_queue,
)
from port_agent_bridge.bridge_api import (
    BERTH_MANAGER_TOOLS,
    BridgeAPI,
)
from port_agent.llm import DryRunProvider


def _make_store_with_holds(hold_nums=(1, 2, 3)):
    """Create a store with multiple hold team states for testing."""
    store = HiveStateStore()
    for n in hold_nums:
        hid = f"hold-{n}"
        create_team_state(store, hid)
        create_container_queue(store, hid, create_sample_containers(count=10))
        create_transport_queue(store, hid)
    return store


def _make_berth_api(store):
    """Create a berth manager BridgeAPI."""
    # Create berth manager entity doc
    store.create_document(
        collection="node_states",
        doc_id="sim_doc_berth-mgr-1",
        fields={
            "node_id": "berth-mgr-1",
            "entity_type": "berth_manager",
            "hive_level": "H3",
            "operational_status": "OPERATIONAL",
            "metrics": {
                "summaries_produced": 0,
                "events_emitted": 0,
                "rebalance_requests": 0,
                "worker_reassignments": 0,
                "scheduler_escalations": 0,
                "hold_priority_changes": 0,
            },
        },
    )
    return BridgeAPI(store, "berth-mgr-1", role="berth_manager")


class TestBerthManagerTools(unittest.TestCase):
    """Test that new tools are registered in BERTH_MANAGER_TOOLS."""

    def test_reassign_worker_tool_exists(self):
        names = [t.name for t in BERTH_MANAGER_TOOLS]
        self.assertIn("reassign_worker", names)

    def test_escalate_to_scheduler_tool_exists(self):
        names = [t.name for t in BERTH_MANAGER_TOOLS]
        self.assertIn("escalate_to_scheduler", names)

    def test_update_hold_priority_tool_exists(self):
        names = [t.name for t in BERTH_MANAGER_TOOLS]
        self.assertIn("update_hold_priority", names)

    def test_berth_manager_has_six_tools(self):
        self.assertEqual(len(BERTH_MANAGER_TOOLS), 6)


class TestReassignWorkerTool(unittest.TestCase):
    """Test the reassign_worker tool handler."""

    def test_reassign_worker_emits_event(self):
        store = _make_store_with_holds()
        api = _make_berth_api(store)
        result = asyncio.get_event_loop().run_until_complete(
            api.call_tool("reassign_worker", {
                "worker_id": "op-2",
                "from_hold": "hold-1",
                "to_hold": "hold-3",
                "reason": "Workforce gap in hold-3",
            })
        )
        text = result.content[0].text
        self.assertIn("op-2", text)
        self.assertIn("hold-1", text)
        self.assertIn("hold-3", text)

        # Verify event was emitted
        events = store.get_events(event_type="worker_reassigned")
        self.assertEqual(len(events), 1)
        evt = events[0]
        self.assertEqual(evt["worker_id"], "op-2")
        self.assertEqual(evt["from_hold"], "hold-1")
        self.assertEqual(evt["to_hold"], "hold-3")
        self.assertEqual(evt["priority"], "HIGH")

    def test_reassign_worker_increments_metric(self):
        store = _make_store_with_holds()
        api = _make_berth_api(store)
        asyncio.get_event_loop().run_until_complete(
            api.call_tool("reassign_worker", {
                "worker_id": "op-1",
                "from_hold": "hold-2",
                "to_hold": "hold-1",
                "reason": "test",
            })
        )
        entity = store.get_document("node_states", "sim_doc_berth-mgr-1")
        self.assertEqual(entity.get_field("metrics.worker_reassignments"), 1)


class TestEscalateToSchedulerTool(unittest.TestCase):
    """Test the escalate_to_scheduler tool handler."""

    def test_escalation_emits_event(self):
        store = _make_store_with_holds()
        api = _make_berth_api(store)
        result = asyncio.get_event_loop().run_until_complete(
            api.call_tool("escalate_to_scheduler", {
                "issue_type": "crane_failure",
                "details": "Crane-2 failed in hold-2, stow plan change needed",
            })
        )
        text = result.content[0].text
        self.assertIn("crane_failure", text)
        self.assertIn("CRITICAL", text)

        events = store.get_events(event_type="scheduler_escalation")
        self.assertEqual(len(events), 1)
        evt = events[0]
        self.assertEqual(evt["issue_type"], "crane_failure")
        self.assertEqual(evt["priority"], "CRITICAL")

    def test_escalation_increments_metric(self):
        store = _make_store_with_holds()
        api = _make_berth_api(store)
        asyncio.get_event_loop().run_until_complete(
            api.call_tool("escalate_to_scheduler", {
                "issue_type": "persistent_throughput_gap",
                "details": "test",
            })
        )
        entity = store.get_document("node_states", "sim_doc_berth-mgr-1")
        self.assertEqual(entity.get_field("metrics.scheduler_escalations"), 1)


class TestUpdateHoldPriorityTool(unittest.TestCase):
    """Test the update_hold_priority tool handler."""

    def test_priority_change_emits_event(self):
        store = _make_store_with_holds()
        api = _make_berth_api(store)
        result = asyncio.get_event_loop().run_until_complete(
            api.call_tool("update_hold_priority", {
                "hold_num": 2,
                "priority_level": "HIGH",
                "reason": "Degraded equipment",
            })
        )
        text = result.content[0].text
        self.assertIn("hold-2", text)
        self.assertIn("HIGH", text)

        events = store.get_events(event_type="hold_priority_changed")
        self.assertEqual(len(events), 1)
        evt = events[0]
        self.assertEqual(evt["hold_id"], "hold-2")
        self.assertEqual(evt["priority_level"], "HIGH")

    def test_priority_change_updates_team_doc(self):
        store = _make_store_with_holds()
        api = _make_berth_api(store)
        asyncio.get_event_loop().run_until_complete(
            api.call_tool("update_hold_priority", {
                "hold_num": 1,
                "priority_level": "CRITICAL",
            })
        )
        team = store.get_document("team_summaries", "team_hold-1")
        self.assertEqual(team.get_field("priority"), "CRITICAL")


class TestBerthManagerTasking(unittest.TestCase):
    """Test enhanced berth manager tasking includes new fields."""

    def test_tasking_includes_worker_counts(self):
        store = _make_store_with_holds()
        api = _make_berth_api(store)
        # Add some team members to hold-1
        team = store.get_document("team_summaries", "team_hold-1")
        team.update_field("team_members", {
            "crane-1": {"entity_type": "gantry_crane", "status": "OPERATIONAL"},
            "op-1": {"entity_type": "operator", "status": "AVAILABLE"},
            "tractor-1": {"entity_type": "yard_tractor", "status": "OPERATIONAL"},
        })

        result = asyncio.get_event_loop().run_until_complete(
            api.read_resource("hive://tasking")
        )
        tasking = json.loads(result.contents[0].text)
        holds = tasking["hold_summaries"]
        # Find hold-1
        h1 = next(h for h in holds if h["hold_id"] == "hold-1")
        self.assertIn("worker_counts", h1)
        self.assertEqual(h1["worker_counts"]["gantry_crane"], 1)
        self.assertEqual(h1["worker_counts"]["operator"], 1)
        self.assertEqual(h1["worker_counts"]["yard_tractor"], 1)

    def test_tasking_includes_degraded_equipment(self):
        store = _make_store_with_holds()
        api = _make_berth_api(store)
        team = store.get_document("team_summaries", "team_hold-2")
        team.update_field("team_members", {
            "crane-3": {"entity_type": "gantry_crane", "status": "DEGRADED"},
        })

        result = asyncio.get_event_loop().run_until_complete(
            api.read_resource("hive://tasking")
        )
        tasking = json.loads(result.contents[0].text)
        h2 = next(h for h in tasking["hold_summaries"] if h["hold_id"] == "hold-2")
        self.assertIn("degraded_equipment", h2)
        self.assertEqual(h2["degraded_equipment"]["crane-3"], "DEGRADED")

    def test_tasking_includes_hold_priority(self):
        store = _make_store_with_holds()
        api = _make_berth_api(store)
        team = store.get_document("team_summaries", "team_hold-3")
        team.update_field("priority", "HIGH")

        result = asyncio.get_event_loop().run_until_complete(
            api.read_resource("hive://tasking")
        )
        tasking = json.loads(result.contents[0].text)
        h3 = next(h for h in tasking["hold_summaries"] if h["hold_id"] == "hold-3")
        self.assertEqual(h3["priority"], "HIGH")

    def test_tasking_includes_gap_details(self):
        store = _make_store_with_holds()
        api = _make_berth_api(store)
        team = store.get_document("team_summaries", "team_hold-1")
        team.update_field("gap_analysis", [
            {"capability": "HAZMAT_CERTIFIED_OPERATOR", "reported_by": "crane-1"},
        ])

        result = asyncio.get_event_loop().run_until_complete(
            api.read_resource("hive://tasking")
        )
        tasking = json.loads(result.contents[0].text)
        h1 = next(h for h in tasking["hold_summaries"] if h["hold_id"] == "hold-1")
        self.assertEqual(h1["gap_count"], 1)
        self.assertEqual(len(h1["gap_details"]), 1)


class TestDryRunBerthManagerDecisions(unittest.TestCase):
    """Test DryRunProvider berth manager cause analysis and corrective actions."""

    def test_detects_crane_failure_and_escalates(self):
        provider = DryRunProvider(role="berth_manager")
        # Advance past cycle 5 summary
        provider._cycle = 6

        state = {
            "tasking": {
                "hold_summaries": [
                    {
                        "hold_id": "hold-1",
                        "moves_per_hour": 10,
                        "target_rate": 35,
                        "status": "DEGRADED",
                        "degraded_equipment": {"crane-1": "FAILED"},
                        "gap_count": 0,
                        "worker_counts": {"gantry_crane": 1, "operator": 1},
                        "moves_remaining": 20,
                    },
                    {
                        "hold_id": "hold-2",
                        "moves_per_hour": 30,
                        "target_rate": 35,
                        "status": "ACTIVE",
                        "degraded_equipment": {},
                        "gap_count": 0,
                        "worker_counts": {"gantry_crane": 2, "operator": 2},
                        "moves_remaining": 15,
                    },
                ],
                "total_moves_remaining": 35,
            }
        }
        decision = asyncio.get_event_loop().run_until_complete(
            provider.decide("", state, [])
        )
        self.assertEqual(decision.action, "escalate_to_scheduler")
        self.assertEqual(decision.arguments["issue_type"], "crane_failure")
        self.assertIn("crane-1", decision.arguments["details"])

    def test_detects_degraded_equipment_and_raises_priority(self):
        provider = DryRunProvider(role="berth_manager")
        provider._cycle = 6

        state = {
            "tasking": {
                "hold_summaries": [
                    {
                        "hold_id": "hold-1",
                        "moves_per_hour": 20,
                        "target_rate": 35,
                        "status": "DEGRADED",
                        "degraded_equipment": {"crane-1": "DEGRADED"},
                        "gap_count": 0,
                        "worker_counts": {"gantry_crane": 2, "operator": 2},
                        "moves_remaining": 20,
                    },
                    {
                        "hold_id": "hold-2",
                        "moves_per_hour": 30,
                        "target_rate": 35,
                        "status": "ACTIVE",
                        "degraded_equipment": {},
                        "gap_count": 0,
                        "worker_counts": {"gantry_crane": 2, "operator": 2},
                        "moves_remaining": 15,
                    },
                ],
                "total_moves_remaining": 35,
            }
        }
        decision = asyncio.get_event_loop().run_until_complete(
            provider.decide("", state, [])
        )
        self.assertEqual(decision.action, "update_hold_priority")
        self.assertEqual(decision.arguments["priority_level"], "HIGH")

    def test_detects_workforce_gap_and_reassigns_worker(self):
        provider = DryRunProvider(role="berth_manager")
        provider._cycle = 6

        state = {
            "tasking": {
                "hold_summaries": [
                    {
                        "hold_id": "hold-1",
                        "moves_per_hour": 15,
                        "target_rate": 35,
                        "status": "ACTIVE",
                        "degraded_equipment": {},
                        "gap_count": 0,
                        "worker_counts": {"gantry_crane": 2, "operator": 1},
                        "moves_remaining": 20,
                    },
                    {
                        "hold_id": "hold-2",
                        "moves_per_hour": 30,
                        "target_rate": 35,
                        "status": "ACTIVE",
                        "degraded_equipment": {},
                        "gap_count": 0,
                        "worker_counts": {"gantry_crane": 2, "operator": 3},
                        "moves_remaining": 15,
                    },
                ],
                "total_moves_remaining": 35,
            }
        }
        decision = asyncio.get_event_loop().run_until_complete(
            provider.decide("", state, [])
        )
        self.assertEqual(decision.action, "reassign_worker")
        self.assertEqual(decision.arguments["to_hold"], "hold-1")
        self.assertEqual(decision.arguments["from_hold"], "hold-2")

    def test_detects_tractor_shortage_and_rebalances(self):
        provider = DryRunProvider(role="berth_manager")
        provider._cycle = 6

        state = {
            "tasking": {
                "hold_summaries": [
                    {
                        "hold_id": "hold-1",
                        "moves_per_hour": 20,
                        "target_rate": 35,
                        "status": "ACTIVE",
                        "degraded_equipment": {},
                        "gap_count": 0,
                        "worker_counts": {"gantry_crane": 2, "operator": 2, "yard_tractor": 1},
                        "moves_remaining": 20,
                    },
                    {
                        "hold_id": "hold-2",
                        "moves_per_hour": 30,
                        "target_rate": 35,
                        "status": "ACTIVE",
                        "degraded_equipment": {},
                        "gap_count": 0,
                        "worker_counts": {"gantry_crane": 2, "operator": 2, "yard_tractor": 3},
                        "moves_remaining": 15,
                    },
                ],
                "total_moves_remaining": 35,
            }
        }
        decision = asyncio.get_event_loop().run_until_complete(
            provider.decide("", state, [])
        )
        self.assertEqual(decision.action, "request_tractor_rebalance")
        self.assertEqual(decision.arguments["to_hold"], "hold-1")
        self.assertEqual(decision.arguments["from_hold"], "hold-2")

    def test_persistent_gaps_escalate_to_scheduler(self):
        provider = DryRunProvider(role="berth_manager")
        provider._cycle = 6

        state = {
            "tasking": {
                "hold_summaries": [
                    {
                        "hold_id": "hold-1",
                        "moves_per_hour": 30,
                        "target_rate": 35,
                        "status": "ACTIVE",
                        "degraded_equipment": {},
                        "gap_count": 3,
                        "worker_counts": {"gantry_crane": 2, "operator": 2},
                        "moves_remaining": 20,
                    },
                ],
                "total_moves_remaining": 20,
            }
        }
        decision = asyncio.get_event_loop().run_until_complete(
            provider.decide("", state, [])
        )
        self.assertEqual(decision.action, "escalate_to_scheduler")
        self.assertEqual(decision.arguments["issue_type"], "persistent_throughput_gap")

    def test_periodic_summary_still_works(self):
        provider = DryRunProvider(role="berth_manager")
        provider._cycle = 4  # Will be 5 after increment in decide()

        state = {
            "tasking": {
                "hold_summaries": [
                    {"hold_id": "hold-1", "moves_per_hour": 30, "status": "ACTIVE"},
                    {"hold_id": "hold-2", "moves_per_hour": 28, "status": "ACTIVE"},
                ],
                "total_moves_remaining": 20,
            }
        }
        decision = asyncio.get_event_loop().run_until_complete(
            provider.decide("", state, [])
        )
        self.assertEqual(decision.action, "update_berth_summary")

    def test_periodic_rebalance_uses_actual_rates(self):
        provider = DryRunProvider(role="berth_manager")
        provider._cycle = 6  # Will be 7 after increment

        state = {
            "tasking": {
                "hold_summaries": [
                    {
                        "hold_id": "hold-1",
                        "moves_per_hour": 35,
                        "target_rate": 35,
                        "status": "ACTIVE",
                        "degraded_equipment": {},
                        "gap_count": 0,
                        "worker_counts": {"gantry_crane": 2, "operator": 2},
                        "moves_remaining": 10,
                    },
                    {
                        "hold_id": "hold-2",
                        "moves_per_hour": 25,
                        "target_rate": 35,
                        "status": "ACTIVE",
                        "degraded_equipment": {},
                        "gap_count": 0,
                        "worker_counts": {"gantry_crane": 2, "operator": 2, "yard_tractor": 2},
                        "moves_remaining": 10,
                    },
                ],
                "total_moves_remaining": 20,
            }
        }
        decision = asyncio.get_event_loop().run_until_complete(
            provider.decide("", state, [])
        )
        self.assertEqual(decision.action, "request_tractor_rebalance")
        # Should rebalance from fastest to slowest
        self.assertEqual(decision.arguments["from_hold"], "hold-1")
        self.assertEqual(decision.arguments["to_hold"], "hold-2")


class TestFindSurplusHold(unittest.TestCase):
    """Test the _find_surplus_hold static method."""

    def test_finds_hold_with_surplus_operators(self):
        holds = [
            {"hold_id": "hold-1", "worker_counts": {"operator": 1}},
            {"hold_id": "hold-2", "worker_counts": {"operator": 3}},
            {"hold_id": "hold-3", "worker_counts": {"operator": 2}},
        ]
        result = DryRunProvider._find_surplus_hold(holds, "hold-1", "operator")
        self.assertEqual(result, "hold-2")

    def test_returns_none_when_no_surplus(self):
        holds = [
            {"hold_id": "hold-1", "worker_counts": {"operator": 1}},
            {"hold_id": "hold-2", "worker_counts": {"operator": 1}},
        ]
        result = DryRunProvider._find_surplus_hold(holds, "hold-1", "operator")
        self.assertIsNone(result)

    def test_excludes_requesting_hold(self):
        holds = [
            {"hold_id": "hold-1", "worker_counts": {"operator": 5}},
            {"hold_id": "hold-2", "worker_counts": {"operator": 1}},
        ]
        result = DryRunProvider._find_surplus_hold(holds, "hold-1", "operator")
        self.assertIsNone(result)


if __name__ == "__main__":
    unittest.main()
