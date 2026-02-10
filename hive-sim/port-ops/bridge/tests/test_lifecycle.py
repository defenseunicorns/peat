"""Unit tests for the capability lifecycle engine."""

from port_agent_bridge.lifecycle import (
    DegradationTracker,
    ResourceTracker,
    CertTracker,
    LifecycleManager,
    SUBSYSTEM_DECAY,
    NOMINAL_THRESHOLD,
    DEGRADED_THRESHOLD,
    _health_status,
)


# ---------------------------------------------------------------------------
#  DegradationTracker
# ---------------------------------------------------------------------------

class TestDegradationTracker:
    def test_initial_confidence_is_one(self):
        dt = DegradationTracker("crane-1")
        assert dt.overall_confidence == 1.0
        for v in dt.subsystems.values():
            assert v == 1.0

    def test_decay_on_container_move(self):
        dt = DegradationTracker("crane-1")
        events = dt.tick("complete_container_move", cycle=5)
        # All subsystems should have decayed
        for sub, rate in SUBSYSTEM_DECAY.items():
            assert dt.subsystems[sub] == 1.0 - rate
        # Should emit events (cycle 5 is divisible by 5)
        assert len(events) > 0
        assert all(e["event_type"] == "CAPABILITY_DEGRADED" for e in events)

    def test_no_decay_on_wait(self):
        dt = DegradationTracker("crane-1")
        events = dt.tick("wait", cycle=1)
        assert len(events) == 0
        assert dt.overall_confidence == 1.0

    def test_threshold_crossing_emits_event(self):
        dt = DegradationTracker("crane-1")
        # Force hydraulic just above threshold
        dt.subsystems["hydraulic"] = NOMINAL_THRESHOLD + 0.01
        events = dt.tick("complete_container_move", cycle=1)
        # hydraulic should cross below NOMINAL_THRESHOLD
        hydro_events = [e for e in events if e["details"]["subsystem"] == "hydraulic"]
        assert len(hydro_events) > 0
        assert hydro_events[0]["details"]["status"] == "DEGRADED"

    def test_restore_increases_confidence(self):
        dt = DegradationTracker("crane-1")
        dt.subsystems["hydraulic"] = 0.3
        dt.restore("hydraulic", 0.5)
        assert dt.subsystems["hydraulic"] == 0.8

    def test_restore_caps_at_one(self):
        dt = DegradationTracker("crane-1")
        dt.restore("hydraulic", 2.0)
        assert dt.subsystems["hydraulic"] == 1.0

    def test_overall_confidence_is_min(self):
        dt = DegradationTracker("crane-1")
        dt.subsystems["hydraulic"] = 0.5
        dt.subsystems["spreader"] = 0.8
        dt.subsystems["electrical"] = 0.9
        assert dt.overall_confidence == 0.5


# ---------------------------------------------------------------------------
#  ResourceTracker
# ---------------------------------------------------------------------------

class TestResourceTracker:
    def test_initial_state(self):
        rt = ResourceTracker("crane-1")
        assert rt.state == "OPERATIONAL"
        for r in rt.resources.values():
            assert r.value == 100.0

    def test_consume_on_action(self):
        rt = ResourceTracker("crane-1")
        events = rt.tick("complete_container_move", sim_minutes=1.0)
        consumed = [e for e in events if e["event_type"] == "RESOURCE_CONSUMED"]
        assert len(consumed) == 3  # hydraulic_fluid, battery, fuel
        for r in rt.resources.values():
            assert r.value < 100.0

    def test_no_consume_on_wait(self):
        rt = ResourceTracker("crane-1")
        events = rt.tick("wait", sim_minutes=1.0)
        assert len(events) == 0

    def test_resupply_triggered_at_threshold(self):
        rt = ResourceTracker("crane-1")
        # Set fluid just above warning threshold
        rt.resources["hydraulic_fluid_pct"].value = 26.0
        events = rt.tick("complete_container_move", sim_minutes=10.0)
        resupply = [e for e in events if e["event_type"] == "RESUPPLY_REQUESTED"]
        assert len(resupply) == 1
        assert rt.state == "RESUPPLYING"

    def test_resupply_completes(self):
        rt = ResourceTracker("crane-1")
        rt.state = "RESUPPLYING"
        rt._resupply_complete_at = 15.0
        events = rt.tick("wait", sim_minutes=15.0)
        completed = [e for e in events if e["event_type"] == "RESUPPLY_COMPLETED"]
        assert len(completed) == 1
        assert rt.state == "OPERATIONAL"
        for r in rt.resources.values():
            assert r.value == 100.0


# ---------------------------------------------------------------------------
#  CertTracker
# ---------------------------------------------------------------------------

class TestCertTracker:
    def test_expiring_warning(self):
        ct = CertTracker("op-1", cert_hours=21.0)
        # Tick 2 hours (120 sim minutes)
        events = ct.tick(sim_minutes=120.0)
        assert ct.cert_hours_remaining < 21.0
        expiring = [e for e in events if e["event_type"] == "CERTIFICATION_EXPIRING"]
        assert len(expiring) == 1

    def test_expired(self):
        ct = CertTracker("op-1", cert_hours=1.0)
        events = ct.tick(sim_minutes=61.0)
        expired = [e for e in events if e["event_type"] == "CERTIFICATION_EXPIRED"]
        assert len(expired) == 1
        assert ct.expired is True


# ---------------------------------------------------------------------------
#  LifecycleManager integration
# ---------------------------------------------------------------------------

class TestLifecycleManager:
    def test_tick_returns_events(self):
        mgr = LifecycleManager("crane-1", report_every_n_cycles=5)
        events = mgr.tick(
            cycle=5,
            action="complete_container_move",
            node_id="crane-1",
            sim_minutes=7.5,
        )
        assert isinstance(events, list)
        assert len(events) > 0
        # All events should have required fields
        for evt in events:
            assert "event_type" in evt
            assert "source" in evt
            assert "priority" in evt
            assert "details" in evt

    def test_tick_no_events_on_wait(self):
        mgr = LifecycleManager("crane-1")
        events = mgr.tick(cycle=1, action="wait", node_id="crane-1", sim_minutes=1.5)
        # May get cert events but no degradation/resource events
        degradation = [e for e in events if e["event_type"] == "CAPABILITY_DEGRADED"]
        resources = [e for e in events if e["event_type"] == "RESOURCE_CONSUMED"]
        assert len(degradation) == 0
        assert len(resources) == 0

    def test_gap_report_emitted(self):
        mgr = LifecycleManager("crane-1", report_every_n_cycles=5)
        # Run enough cycles to trigger degradation + gap report
        all_events = []
        for i in range(1, 11):
            evts = mgr.tick(
                cycle=i,
                action="complete_container_move",
                node_id="crane-1",
                sim_minutes=i * 1.5,
            )
            all_events.extend(evts)
        gap_reports = [e for e in all_events if e["event_type"] == "GAP_ANALYSIS_REPORT"]
        assert len(gap_reports) >= 1
        report = gap_reports[0]
        assert "readiness_score" in report["details"]

    def test_health_status_thresholds(self):
        assert _health_status(1.0) == "NOMINAL"
        assert _health_status(0.7) == "NOMINAL"
        assert _health_status(0.69) == "DEGRADED"
        assert _health_status(0.4) == "DEGRADED"
        assert _health_status(0.39) == "CRITICAL"
        assert _health_status(0.0) == "OFFLINE"
