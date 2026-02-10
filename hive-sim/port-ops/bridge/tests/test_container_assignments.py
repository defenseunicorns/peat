"""Unit tests for container-level assignment and tracking (hi-qnmf)."""

import asyncio
import pytest

from port_agent_bridge.hive_state import (
    HiveStateStore,
    create_container_queue,
    create_team_state,
    create_sample_containers,
    create_transport_queue,
)


@pytest.fixture
def store():
    s = HiveStateStore()
    containers = create_sample_containers(count=5, hazmat_count=1)
    create_container_queue(s, "hold-3", containers)
    create_team_state(s, "hold-3")
    create_transport_queue(s, "hold-3")
    s.create_container_assignments("hold-3")
    return s


class TestContainerAssignments:
    def test_create_assignments_doc(self, store):
        doc = store.get_document("container_assignments", "assignments_hold-3")
        assert doc is not None
        assert doc.fields["hold_id"] == "hold-3"
        assert doc.fields["assignments"] == {}

    def test_assign_container(self, store):
        ok = store.assign_container(
            "hold-3", "MSCU-4472891",
            assigned_crane="crane-1",
            assigned_operator="op-1",
            assigned_tractor="tractor-1",
        )
        assert ok
        rec = store.get_container_assignment("hold-3", "MSCU-4472891")
        assert rec is not None
        assert rec["assigned_crane"] == "crane-1"
        assert rec["assigned_operator"] == "op-1"
        assert rec["assigned_tractor"] == "tractor-1"
        assert rec["status"] == "QUEUED"

    def test_assign_emits_event(self, store):
        store.assign_container(
            "hold-3", "MSCU-4472891", assigned_crane="crane-1",
        )
        events = store.get_events(event_type="container_assignment")
        assert len(events) == 1
        assert events[0]["container_id"] == "MSCU-4472891"
        assert events[0]["assigned_crane"] == "crane-1"

    def test_update_container_status(self, store):
        store.assign_container(
            "hold-3", "MSCU-4472891", assigned_crane="crane-1",
        )
        ok = store.update_container_status("hold-3", "MSCU-4472891", "IN_PROGRESS")
        assert ok
        rec = store.get_container_assignment("hold-3", "MSCU-4472891")
        assert rec["status"] == "IN_PROGRESS"

    def test_update_nonexistent_container(self, store):
        ok = store.update_container_status("hold-3", "NONEXISTENT", "IN_PROGRESS")
        assert not ok

    def test_status_progression(self, store):
        store.assign_container(
            "hold-3", "MSCU-4472891", assigned_crane="crane-1",
        )
        for status in ["IN_PROGRESS", "DISCHARGED", "TRANSPORTED", "SECURED"]:
            assert store.update_container_status("hold-3", "MSCU-4472891", status)
            rec = store.get_container_assignment("hold-3", "MSCU-4472891")
            assert rec["status"] == status

    def test_get_assignments_by_role(self, store):
        store.assign_container(
            "hold-3", "MSCU-4472891", assigned_crane="crane-1",
        )
        store.assign_container(
            "hold-3", "MSCU-4472892", assigned_crane="crane-2",
        )
        store.assign_container(
            "hold-3", "MSCU-4472893", assigned_crane="crane-1",
        )
        crane1_assignments = store.get_container_assignments_by_role(
            "hold-3", "assigned_crane", "crane-1",
        )
        assert len(crane1_assignments) == 2
        ids = {a["container_id"] for a in crane1_assignments}
        assert ids == {"MSCU-4472891", "MSCU-4472893"}

    def test_status_breakdown(self, store):
        store.assign_container(
            "hold-3", "MSCU-4472891", assigned_crane="crane-1",
        )
        store.assign_container(
            "hold-3", "MSCU-4472892", assigned_crane="crane-1",
        )
        store.update_container_status("hold-3", "MSCU-4472891", "DISCHARGED")
        breakdown = store.get_container_status_breakdown("hold-3")
        assert breakdown == {"DISCHARGED": 1, "QUEUED": 1}

    def test_partial_assignment_update(self, store):
        """Assigning a tractor later merges with existing crane assignment."""
        store.assign_container(
            "hold-3", "MSCU-4472891", assigned_crane="crane-1",
        )
        store.assign_container(
            "hold-3", "MSCU-4472891", assigned_tractor="tractor-2",
        )
        rec = store.get_container_assignment("hold-3", "MSCU-4472891")
        assert rec["assigned_crane"] == "crane-1"
        assert rec["assigned_tractor"] == "tractor-2"

    def test_empty_status_breakdown(self, store):
        breakdown = store.get_container_status_breakdown("hold-3")
        assert breakdown == {}

    def test_assign_to_nonexistent_hold(self, store):
        ok = store.assign_container(
            "hold-999", "MSCU-4472891", assigned_crane="crane-1",
        )
        assert not ok


class TestBridgeAPIContainerAssignments:
    """Integration tests for BridgeAPI container assignment features."""

    @pytest.fixture
    def bridge(self, store):
        from port_agent_bridge.bridge_api import BridgeAPI
        return BridgeAPI(store, node_id="crane-1", role="crane", hold_id="hold-3")

    @pytest.fixture
    def scheduler_bridge(self, store):
        from port_agent_bridge.bridge_api import BridgeAPI
        return BridgeAPI(store, node_id="scheduler-1", role="scheduler", hold_id="hold-3")

    def test_crane_tasking_includes_assigned_containers(self, store, bridge):
        store.assign_container(
            "hold-3", "MSCU-4472891", assigned_crane="crane-1",
        )
        import json
        result = asyncio.get_event_loop().run_until_complete(
            bridge.read_resource("hive://tasking")
        )
        tasking = json.loads(result.contents[0].text)
        assert "assigned_containers" in tasking
        assert "MSCU-4472891" in tasking["assigned_containers"]

    def test_assign_container_tool(self, store, scheduler_bridge):
        import json
        result = asyncio.get_event_loop().run_until_complete(
            scheduler_bridge.call_tool("assign_container", {
                "container_id": "MSCU-4472891",
                "assigned_crane": "crane-1",
                "assigned_operator": "op-1",
            })
        )
        text = result.content[0].text
        assert "MSCU-4472891" in text
        assert "crane=crane-1" in text

        rec = store.get_container_assignment("hold-3", "MSCU-4472891")
        assert rec is not None
        assert rec["assigned_crane"] == "crane-1"
        assert rec["assigned_operator"] == "op-1"

    def test_container_assignments_resource(self, store, bridge):
        store.assign_container(
            "hold-3", "MSCU-4472891", assigned_crane="crane-1",
        )
        store.assign_container(
            "hold-3", "MSCU-4472892", assigned_crane="crane-2",
        )
        import json
        result = asyncio.get_event_loop().run_until_complete(
            bridge.read_resource("hive://container-assignments")
        )
        data = json.loads(result.contents[0].text)
        # crane-1 should only see its own assignment
        assert "MSCU-4472891" in data["assignments"]
        assert "MSCU-4472892" not in data["assignments"]
        assert data["total_assigned"] == 2

    def test_aggregator_tasking_includes_breakdown(self, store):
        from port_agent_bridge.bridge_api import BridgeAPI
        agg = BridgeAPI(store, node_id="hold-agg-3", role="aggregator", hold_id="hold-3")
        store.assign_container(
            "hold-3", "MSCU-4472891", assigned_crane="crane-1",
        )
        store.update_container_status("hold-3", "MSCU-4472891", "DISCHARGED")
        import json
        result = asyncio.get_event_loop().run_until_complete(
            agg.read_resource("hive://tasking")
        )
        tasking = json.loads(result.contents[0].text)
        assert "container_status_breakdown" in tasking
        assert tasking["container_status_breakdown"]["DISCHARGED"] == 1
