"""Tests for proficiency levels affecting agent efficiency (hi-ch9y)."""

import asyncio

from port_agent.llm import (
    DryRunProvider,
    PROFICIENCY_MODIFIERS,
    create_provider,
)
from port_agent.orchestrator import (
    AgentSpec,
    PROFICIENCY_LEVELS,
    parse_agent_composition,
)
from port_agent_bridge.bridge_api import BridgeAPI, PROFICIENCY_EXEC_MODIFIERS
from port_agent_bridge.lifecycle import (
    CertTracker,
    LifecycleManager,
    _RECERT_TIME_MULTIPLIER,
)
from port_agent_bridge.hive_state import HiveStateStore


# ---------------------------------------------------------------------------
#  Proficiency constants
# ---------------------------------------------------------------------------

class TestProficiencyConstants:
    def test_four_levels_defined(self):
        assert len(PROFICIENCY_LEVELS) == 4
        assert "novice" in PROFICIENCY_LEVELS
        assert "advanced_beginner" in PROFICIENCY_LEVELS
        assert "competent" in PROFICIENCY_LEVELS
        assert "expert" in PROFICIENCY_LEVELS

    def test_modifiers_for_all_levels(self):
        for level in PROFICIENCY_LEVELS:
            assert level in PROFICIENCY_MODIFIERS
            mods = PROFICIENCY_MODIFIERS[level]
            assert "speed_factor" in mods
            assert "error_rate" in mods

    def test_expert_is_fastest(self):
        for level in PROFICIENCY_LEVELS:
            assert PROFICIENCY_MODIFIERS["expert"]["speed_factor"] <= PROFICIENCY_MODIFIERS[level]["speed_factor"]

    def test_expert_has_zero_error(self):
        assert PROFICIENCY_MODIFIERS["expert"]["error_rate"] == 0.0

    def test_novice_has_highest_error(self):
        for level in PROFICIENCY_LEVELS:
            assert PROFICIENCY_MODIFIERS["novice"]["error_rate"] >= PROFICIENCY_MODIFIERS[level]["error_rate"]


# ---------------------------------------------------------------------------
#  Orchestrator: AgentSpec proficiency
# ---------------------------------------------------------------------------

class TestAgentSpecProficiency:
    def test_default_proficiency_is_competent(self):
        spec = AgentSpec(
            node_id="test-1", role="crane", persona="gantry-crane",
            provider="dry-run",
        )
        assert spec.proficiency == "competent"

    def test_explicit_proficiency(self):
        spec = AgentSpec(
            node_id="test-1", role="crane", persona="gantry-crane",
            provider="dry-run", proficiency="novice",
        )
        assert spec.proficiency == "novice"


# ---------------------------------------------------------------------------
#  Orchestrator: composition proficiency assignment
# ---------------------------------------------------------------------------

class TestCompositionProficiency:
    def test_singleton_gets_expert(self):
        specs = parse_agent_composition("1s", provider="dry-run")
        assert len(specs) == 1
        assert specs[0].proficiency == "expert"

    def test_lead_worker_gets_expert(self):
        specs = parse_agent_composition("3w", provider="dry-run")
        assert specs[0].proficiency == "expert"  # lead (op-1)

    def test_subsequent_workers_get_mixed(self):
        specs = parse_agent_composition("5w", provider="dry-run")
        assert specs[0].proficiency == "expert"
        # Remaining 4 follow the mixed pattern
        assert specs[1].proficiency == "competent"
        assert specs[2].proficiency == "advanced_beginner"
        assert specs[3].proficiency == "novice"
        assert specs[4].proficiency == "competent"

    def test_full_composition_has_varied_proficiency(self):
        specs = parse_agent_composition("2c5w4t1s1a2x", provider="dry-run")
        proficiencies = {s.proficiency for s in specs}
        # Should have at least expert and one other level
        assert "expert" in proficiencies
        assert len(proficiencies) > 1

    def test_all_proficiencies_are_valid(self):
        specs = parse_agent_composition("2c5w4t1s1a2x", provider="dry-run")
        for s in specs:
            assert s.proficiency in PROFICIENCY_LEVELS


# ---------------------------------------------------------------------------
#  LLM: DryRunProvider proficiency
# ---------------------------------------------------------------------------

class TestDryRunProviderProficiency:
    def test_default_proficiency(self):
        provider = DryRunProvider(role="crane")
        assert provider._proficiency == "competent"

    def test_explicit_proficiency(self):
        provider = DryRunProvider(role="crane", proficiency="novice")
        assert provider._proficiency == "novice"
        assert provider._speed_factor == PROFICIENCY_MODIFIERS["novice"]["speed_factor"]
        assert provider._error_rate == PROFICIENCY_MODIFIERS["novice"]["error_rate"]

    def test_expert_no_errors(self):
        provider = DryRunProvider(role="crane", proficiency="expert")
        assert provider._error_rate == 0.0

    def test_create_provider_passes_proficiency(self):
        provider = create_provider("dry-run", role="crane", proficiency="novice")
        assert isinstance(provider, DryRunProvider)
        assert provider._proficiency == "novice"

    def test_decide_returns_valid_action(self):
        provider = DryRunProvider(role="crane", proficiency="expert")
        decision = asyncio.get_event_loop().run_until_complete(
            provider.decide(
                persona="test",
                observed_state={"container_queue": {"next_containers": []}},
                available_tools=[],
            )
        )
        assert decision.action in ("wait", "complete_container_move", "request_support")


# ---------------------------------------------------------------------------
#  BridgeAPI: proficiency in tasking
# ---------------------------------------------------------------------------

class TestBridgeAPIProficiency:
    def test_bridge_stores_proficiency(self):
        store = HiveStateStore()
        bridge = BridgeAPI(store=store, node_id="crane-1", proficiency="novice")
        assert bridge.proficiency == "novice"
        assert bridge._exec_modifier == PROFICIENCY_EXEC_MODIFIERS["novice"]

    def test_exec_modifiers_defined_for_all_levels(self):
        for level in PROFICIENCY_LEVELS:
            assert level in PROFICIENCY_EXEC_MODIFIERS


# ---------------------------------------------------------------------------
#  Lifecycle: CertTracker proficiency
# ---------------------------------------------------------------------------

class TestCertTrackerProficiency:
    def test_default_proficiency(self):
        ct = CertTracker("op-1")
        assert ct.proficiency == "competent"

    def test_expert_recertifies_fastest(self):
        ct_expert = CertTracker("op-1", cert_hours=1.0, proficiency="expert")
        ct_novice = CertTracker("op-2", cert_hours=1.0, proficiency="novice")

        # Expire both
        events_expert = ct_expert.tick(sim_minutes=61.0)
        events_novice = ct_novice.tick(sim_minutes=61.0)

        expired_expert = [e for e in events_expert if e["event_type"] == "CERTIFICATION_EXPIRED"]
        expired_novice = [e for e in events_novice if e["event_type"] == "CERTIFICATION_EXPIRED"]

        assert len(expired_expert) == 1
        assert len(expired_novice) == 1

        # Expert recerts faster than novice
        expert_eta = expired_expert[0]["details"]["recert_eta_sim_minutes"]
        novice_eta = expired_novice[0]["details"]["recert_eta_sim_minutes"]
        assert expert_eta < novice_eta

    def test_proficiency_in_events(self):
        ct = CertTracker("op-1", cert_hours=1.0, proficiency="advanced_beginner")
        events = ct.tick(sim_minutes=61.0)
        expired = [e for e in events if e["event_type"] == "CERTIFICATION_EXPIRED"]
        assert expired[0]["details"]["proficiency"] == "advanced_beginner"

    def test_recert_time_multipliers(self):
        base = 10.0
        for level, mult in _RECERT_TIME_MULTIPLIER.items():
            ct = CertTracker("test", cert_hours=1.0, proficiency=level)
            events = ct.tick(sim_minutes=61.0)
            expired = [e for e in events if e["event_type"] == "CERTIFICATION_EXPIRED"]
            assert expired[0]["details"]["recert_eta_sim_minutes"] == base * mult


# ---------------------------------------------------------------------------
#  Lifecycle: LifecycleManager with proficiency
# ---------------------------------------------------------------------------

class TestLifecycleManagerProficiency:
    def test_accepts_proficiency(self):
        mgr = LifecycleManager("crane-1", role="crane", proficiency="novice")
        assert mgr.proficiency == "novice"

    def test_cert_tracker_inherits_proficiency(self):
        mgr = LifecycleManager("op-1", role="operator", proficiency="expert")
        assert mgr.certs is not None
        assert mgr.certs.proficiency == "expert"

    def test_default_proficiency_is_competent(self):
        mgr = LifecycleManager("crane-1")
        assert mgr.proficiency == "competent"
