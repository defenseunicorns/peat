"""Tests for tiered LLM runtime configuration.

Validates Addendum A §Technical Implementation (Option C: Tiered):
- TOML config loading and parsing
- Role → tier → provider mapping
- HybridProvider escalation logic
- Different agents use different LLM tiers in same simulation
- API calls only for complex/coordination agents (cost tracking)
"""

import asyncio
import json
import os
import tempfile

import pytest

from port_agent.llm import (
    AnthropicProvider,
    DryRunProvider,
    HybridProvider,
    OllamaProvider,
    TierConfig,
    create_provider,
    create_provider_from_tier,
    load_tier_config,
)


def _has_openai():
    try:
        import openai
        return True
    except ImportError:
        return False


def _has_anthropic():
    try:
        import anthropic
        return True
    except ImportError:
        return False


SAMPLE_CONFIG = """\
[tiers.rule_based]
provider = "dry-run"
description = "Deterministic rule-based decisions"

[tiers.local_slm]
provider = "ollama"
model = "qwen3:1.7b"
base_url = "http://localhost:11434/v1"
description = "Local SLM for equipment"

[tiers.api]
provider = "anthropic"
model = "claude-sonnet-4-5-20250929"
description = "Capable model API for coordination"

[tiers.hybrid]
provider = "ollama"
model = "qwen3:1.7b"
base_url = "http://localhost:11434/v1"
escalation_provider = "anthropic"
escalation_model = "claude-sonnet-4-5-20250929"
escalation_keywords = ["hazmat", "certification", "safety"]
description = "SLM for routine, API for complex"

[roles]
sensor        = "rule_based"
crane         = "local_slm"
tractor       = "local_slm"
operator      = "hybrid"
lashing_crew  = "hybrid"
signaler      = "hybrid"
aggregator    = "api"
berth_manager = "api"
scheduler     = "api"
"""


@pytest.fixture
def config_path(tmp_path):
    """Write sample TOML config to a temp file."""
    path = tmp_path / "llm-tiers.toml"
    path.write_text(SAMPLE_CONFIG)
    return str(path)


@pytest.fixture
def tier_config(config_path):
    return load_tier_config(config_path)


class TestLoadTierConfig:
    """Tests for TOML config parsing."""

    def test_loads_tiers(self, tier_config):
        assert "rule_based" in tier_config.tiers
        assert "local_slm" in tier_config.tiers
        assert "api" in tier_config.tiers
        assert "hybrid" in tier_config.tiers

    def test_loads_role_mapping(self, tier_config):
        assert tier_config.role_mapping["sensor"] == "rule_based"
        assert tier_config.role_mapping["crane"] == "local_slm"
        assert tier_config.role_mapping["aggregator"] == "api"
        assert tier_config.role_mapping["operator"] == "hybrid"

    def test_tier_has_provider(self, tier_config):
        assert tier_config.tiers["rule_based"]["provider"] == "dry-run"
        assert tier_config.tiers["local_slm"]["provider"] == "ollama"
        assert tier_config.tiers["api"]["provider"] == "anthropic"

    def test_hybrid_has_escalation(self, tier_config):
        hybrid = tier_config.tiers["hybrid"]
        assert hybrid["escalation_provider"] == "anthropic"
        assert "hazmat" in hybrid["escalation_keywords"]

    def test_missing_file_raises(self):
        with pytest.raises(FileNotFoundError):
            load_tier_config("/nonexistent/path.toml")


class TestCreateProviderFromTier:
    """Tests for creating providers from tier config."""

    def test_sensor_gets_dry_run(self, tier_config):
        provider = create_provider_from_tier(tier_config, "sensor")
        assert isinstance(provider, DryRunProvider)

    @pytest.mark.skipif(
        not _has_openai(), reason="openai package not installed"
    )
    def test_crane_gets_ollama(self, tier_config):
        provider = create_provider_from_tier(tier_config, "crane")
        assert isinstance(provider, OllamaProvider)

    @pytest.mark.skipif(
        not _has_openai(), reason="openai package not installed"
    )
    def test_tractor_gets_ollama(self, tier_config):
        provider = create_provider_from_tier(tier_config, "tractor")
        assert isinstance(provider, OllamaProvider)

    @pytest.mark.skipif(
        not _has_anthropic(), reason="anthropic package not installed"
    )
    def test_aggregator_gets_anthropic(self, tier_config):
        provider = create_provider_from_tier(tier_config, "aggregator")
        assert isinstance(provider, AnthropicProvider)

    @pytest.mark.skipif(
        not _has_anthropic(), reason="anthropic package not installed"
    )
    def test_berth_manager_gets_anthropic(self, tier_config):
        provider = create_provider_from_tier(tier_config, "berth_manager")
        assert isinstance(provider, AnthropicProvider)

    @pytest.mark.skipif(
        not (_has_openai() and _has_anthropic()),
        reason="openai and/or anthropic packages not installed",
    )
    def test_operator_gets_hybrid(self, tier_config):
        provider = create_provider_from_tier(tier_config, "operator")
        assert isinstance(provider, HybridProvider)
        assert isinstance(provider.slm, OllamaProvider)
        assert isinstance(provider.api, AnthropicProvider)

    def test_unknown_role_falls_back_to_dry_run(self, tier_config):
        provider = create_provider_from_tier(tier_config, "unknown_role")
        assert isinstance(provider, DryRunProvider)

    def test_undefined_tier_raises(self, tier_config):
        tier_config.role_mapping["crane"] = "nonexistent_tier"
        with pytest.raises(ValueError, match="not defined in config"):
            create_provider_from_tier(tier_config, "crane")


class TestHybridProvider:
    """Tests for the hybrid SLM/API escalation provider."""

    def test_routine_state_uses_slm(self):
        slm = DryRunProvider(role="operator")
        api = DryRunProvider(role="operator")
        hybrid = HybridProvider(slm=slm, api=api)

        state = {"tasking": {"status": "AVAILABLE"}, "container_queue": {"next_containers": []}}
        assert not hybrid._needs_escalation(state)

    def test_hazmat_state_triggers_escalation(self):
        slm = DryRunProvider(role="operator")
        api = DryRunProvider(role="operator")
        hybrid = HybridProvider(slm=slm, api=api)

        state = {
            "tasking": {"status": "AVAILABLE"},
            "container_queue": {
                "next_containers": [
                    {"container_id": "MSCU-001", "hazmat": True, "hazmat_class": 3},
                ],
            },
        }
        assert hybrid._needs_escalation(state)

    def test_certification_state_triggers_escalation(self):
        slm = DryRunProvider(role="operator")
        api = DryRunProvider(role="operator")
        hybrid = HybridProvider(slm=slm, api=api)

        state = {"tasking": {"certification": "expired", "needs_recertification": True}}
        assert hybrid._needs_escalation(state)

    def test_custom_keywords(self):
        slm = DryRunProvider(role="operator")
        api = DryRunProvider(role="operator")
        hybrid = HybridProvider(slm=slm, api=api, escalation_keywords=["overweight"])

        state = {"tasking": {"container_weight": "overweight"}}
        assert hybrid._needs_escalation(state)

        state2 = {"tasking": {"status": "AVAILABLE"}}
        assert not hybrid._needs_escalation(state2)

    def test_escalation_counter(self):
        slm = DryRunProvider(role="operator")
        api = DryRunProvider(role="operator")
        hybrid = HybridProvider(slm=slm, api=api)

        routine = {"tasking": {"status": "AVAILABLE"}}
        complex_ = {"tasking": {"hazmat": True}}

        asyncio.get_event_loop().run_until_complete(
            hybrid.decide("persona", routine, [])
        )
        assert hybrid.slm_calls == 1
        assert hybrid.api_calls == 0

        asyncio.get_event_loop().run_until_complete(
            hybrid.decide("persona", complex_, [])
        )
        assert hybrid.slm_calls == 1
        assert hybrid.api_calls == 1


class TestTierDiversity:
    """Validates that different agents use different tiers in same simulation."""

    def test_all_roles_have_tier_mapping(self, tier_config):
        """Every known role has a tier assignment."""
        expected_roles = [
            "sensor", "crane", "tractor", "operator",
            "lashing_crew", "signaler", "aggregator",
            "berth_manager", "scheduler",
        ]
        for role in expected_roles:
            assert role in tier_config.role_mapping, f"Missing tier mapping for {role}"

    def test_multiple_tier_types_used(self, tier_config):
        """Config uses at least 3 different tiers (validates diversity)."""
        unique_tiers = set(tier_config.role_mapping.values())
        assert len(unique_tiers) >= 3, f"Only {unique_tiers} — need at least 3 tiers"

    def test_sensors_are_rule_based(self, tier_config):
        """H0 sensors must be rule_based (no LLM)."""
        assert tier_config.role_mapping["sensor"] == "rule_based"

    def test_equipment_uses_slm(self, tier_config):
        """H1 equipment must use local_slm."""
        assert tier_config.role_mapping["crane"] == "local_slm"
        assert tier_config.role_mapping["tractor"] == "local_slm"

    def test_coordination_uses_api(self, tier_config):
        """H2+ coordination must use api."""
        assert tier_config.role_mapping["aggregator"] == "api"
        assert tier_config.role_mapping["berth_manager"] == "api"
        assert tier_config.role_mapping["scheduler"] == "api"

    def test_workers_use_hybrid(self, tier_config):
        """Workers use hybrid (SLM for routine, API for complex)."""
        assert tier_config.role_mapping["operator"] == "hybrid"
        assert tier_config.role_mapping["lashing_crew"] == "hybrid"
        assert tier_config.role_mapping["signaler"] == "hybrid"

    def test_api_only_for_complex_agents(self, tier_config):
        """Only coordination and hybrid tiers should make API calls.
        Rule-based and SLM tiers should never call remote APIs."""
        api_tiers = {"api", "hybrid"}
        non_api_roles = [
            role for role, tier in tier_config.role_mapping.items()
            if tier not in api_tiers
        ]
        # Sensors, cranes, tractors should not use API
        assert "sensor" in non_api_roles
        assert "crane" in non_api_roles
        assert "tractor" in non_api_roles
