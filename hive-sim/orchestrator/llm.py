"""
LLM provider abstraction and dry-run decision engine for port operations simulation.

Supports tiered LLM runtime per Addendum A §LLM Runtime Options (Option C: Tiered):
- dry-run: No LLM, returns canned responses (for testing)
- local_slm: Local small model via Ollama (for equipment agents)
- api: Remote API provider (for sophisticated reasoning)

Equipment agents (cranes, tractors) use local_slm for low-latency,
no-external-dependency reasoning suitable for lift sequencing and route decisions.

Phase2-dry mode uses DryRunDecisionEngine for deterministic, rule-based decisions
that mirror what an LLM would produce during live operation.
"""

import json
import logging
import time
import urllib.error
import urllib.request
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Optional

logger = logging.getLogger(__name__)


@dataclass
class LlmResponse:
    """Response from an LLM provider."""
    text: str
    model: str
    provider: str
    latency_ms: float
    tokens_generated: int = 0


class LlmProvider(ABC):
    """Abstract base class for LLM providers."""

    @abstractmethod
    def generate(self, prompt: str, system: Optional[str] = None, max_tokens: int = 256) -> LlmResponse:
        """Generate a response from the model."""

    @abstractmethod
    def is_ready(self) -> bool:
        """Check if the provider is ready to serve requests."""

    @abstractmethod
    def provider_name(self) -> str:
        """Return the provider name."""


class DryRunProvider(LlmProvider):
    """No-op provider that returns canned responses. For testing and baselines."""

    CANNED_RESPONSES = {
        "crane": "LIFT: Proceed with standard lift sequence. Check load weight, verify clearance, execute hoist.",
        "tractor": "ROUTE: Take shortest available path to destination. Avoid congested lanes.",
        "default": "DECISION: Proceed with default action sequence.",
    }

    def generate(self, prompt: str, system: Optional[str] = None, max_tokens: int = 256) -> LlmResponse:
        start = time.monotonic()
        # Match canned response based on system prompt (equipment type)
        text = self.CANNED_RESPONSES["default"]
        match_text = (system or "").lower()
        for key, response in self.CANNED_RESPONSES.items():
            if key != "default" and key in match_text:
                text = response
                break
        latency = (time.monotonic() - start) * 1000
        return LlmResponse(text=text, model="dry-run", provider="dry-run", latency_ms=latency)

    def is_ready(self) -> bool:
        return True

    def provider_name(self) -> str:
        return "dry-run"


class OllamaProvider(LlmProvider):
    """Local SLM provider via Ollama for equipment agents.

    Connects to an Ollama instance running locally or on the network.
    Suitable models: llama3:8b, mistral:7b, phi3:mini, gemma2:2b.
    """

    def __init__(self, endpoint: str = "http://localhost:11434", model: str = "llama3:8b",
                 timeout: float = 30.0):
        self.endpoint = endpoint.rstrip("/")
        self.model = model
        self.timeout = timeout
        self._ready: Optional[bool] = None

    def generate(self, prompt: str, system: Optional[str] = None, max_tokens: int = 256) -> LlmResponse:
        start = time.monotonic()
        url = f"{self.endpoint}/api/generate"
        payload = {
            "model": self.model,
            "prompt": prompt,
            "stream": False,
            "options": {
                "num_predict": max_tokens,
                "temperature": 0.7,
            },
        }
        if system:
            payload["system"] = system

        data = json.dumps(payload).encode()
        req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})

        try:
            with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                result = json.loads(resp.read().decode())
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            latency = (time.monotonic() - start) * 1000
            logger.error("Ollama request failed: %s", e)
            return LlmResponse(
                text=f"ERROR: Ollama unavailable ({e})",
                model=self.model,
                provider="ollama",
                latency_ms=latency,
            )

        latency = (time.monotonic() - start) * 1000
        text = result.get("response", "")
        # Ollama returns eval_count for tokens generated
        tokens = result.get("eval_count", 0)

        return LlmResponse(
            text=text.strip(),
            model=self.model,
            provider="ollama",
            latency_ms=latency,
            tokens_generated=tokens,
        )

    def is_ready(self) -> bool:
        if self._ready is not None:
            return self._ready
        try:
            url = f"{self.endpoint}/api/tags"
            req = urllib.request.Request(url)
            with urllib.request.urlopen(req, timeout=5.0) as resp:
                data = json.loads(resp.read().decode())
                models = [m.get("name", "") for m in data.get("models", [])]
                self._ready = any(self.model in m for m in models)
                if not self._ready:
                    logger.warning("Model %s not found in Ollama. Available: %s", self.model, models)
                return self._ready
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            logger.warning("Ollama health check failed: %s", e)
            self._ready = False
            return False

    def provider_name(self) -> str:
        return "ollama"


class ApiProvider(LlmProvider):
    """Remote API provider for sophisticated reasoning tasks.

    Placeholder for external LLM API integration (e.g., Claude, OpenAI).
    Not used by equipment agents—included for tiered architecture completeness.
    """

    def __init__(self, api_url: str = "", api_key: str = "", model: str = ""):
        self.api_url = api_url
        self.api_key = api_key
        self.model = model

    def generate(self, prompt: str, system: Optional[str] = None, max_tokens: int = 256) -> LlmResponse:
        # Placeholder—equipment agents use local_slm, not API
        return LlmResponse(
            text="ERROR: API provider not configured",
            model=self.model or "api",
            provider="api",
            latency_ms=0.0,
        )

    def is_ready(self) -> bool:
        return bool(self.api_url and self.api_key)

    def provider_name(self) -> str:
        return "api"


# LLM tier configuration for equipment types
EQUIPMENT_LLM_TIERS: dict[str, str] = {
    "crane": "local_slm",
    "tractor": "local_slm",
    "vessel": "local_slm",
    "quay_crane": "local_slm",
    "rtg_crane": "local_slm",
    "straddle_carrier": "local_slm",
}

# System prompts for equipment agent reasoning
EQUIPMENT_SYSTEM_PROMPTS: dict[str, str] = {
    "crane": (
        "You are a crane operations AI. Make concise lift sequencing decisions. "
        "Consider load weight, clearance, wind conditions, and adjacent operations. "
        "Output a numbered action sequence. Be brief."
    ),
    "tractor": (
        "You are a terminal tractor routing AI. Plan efficient container transport routes. "
        "Consider lane congestion, priority cargo, and dock assignments. "
        "Output the route as a sequence of waypoints. Be brief."
    ),
    "default": (
        "You are a port equipment operations AI. Make safe, efficient operational decisions. "
        "Be concise and actionable."
    ),
}


def create_provider(provider_type: str, **kwargs) -> LlmProvider:
    """Factory function to create LLM providers.

    Args:
        provider_type: One of 'dry-run', 'ollama', 'api'.
        **kwargs: Provider-specific configuration.

    Returns:
        Configured LlmProvider instance.
    """
    if provider_type == "dry-run":
        return DryRunProvider()
    elif provider_type == "ollama":
        return OllamaProvider(
            endpoint=kwargs.get("ollama_endpoint", "http://localhost:11434"),
            model=kwargs.get("ollama_model", "llama3:8b"),
            timeout=kwargs.get("ollama_timeout", 30.0),
        )
    elif provider_type == "api":
        return ApiProvider(
            api_url=kwargs.get("api_url", ""),
            api_key=kwargs.get("api_key", ""),
            model=kwargs.get("api_model", ""),
        )
    else:
        raise ValueError(f"Unknown provider type: {provider_type}. Use 'dry-run', 'ollama', or 'api'.")


def get_system_prompt(equipment_type: str) -> str:
    """Get the system prompt for an equipment type."""
    return EQUIPMENT_SYSTEM_PROMPTS.get(equipment_type, EQUIPMENT_SYSTEM_PROMPTS["default"])


# ---------------------------------------------------------------------------
# Dry-Run Decision Engine (phase2-dry scenario mode)
# ---------------------------------------------------------------------------

from bridge_api import EntityStateManager, EntityStatus


@dataclass
class Decision:
    decision_id: str
    decision_type: str
    action: str
    rationale: str
    confidence: float
    affected_entities: list[str] = field(default_factory=list)
    parameters: dict = field(default_factory=dict)

    def to_dict(self) -> dict:
        return {
            "decision_id": self.decision_id,
            "decision_type": self.decision_type,
            "action": self.action,
            "rationale": self.rationale,
            "confidence": self.confidence,
            "affected_entities": self.affected_entities,
            "parameters": self.parameters,
        }


class DryRunDecisionEngine:
    """Deterministic decision engine for phase2-dry simulation."""

    def __init__(self, state: EntityStateManager):
        self.state = state
        self._decision_counter = 0

    def _next_id(self) -> str:
        self._decision_counter += 1
        return f"dec-{self._decision_counter:04d}"

    def decide_on_event(self, event: dict) -> list[Decision]:
        """Produce decisions based on a scenario event and current state."""
        event_type = event.get("type", "")
        handler = _EVENT_DECISION_MAP.get(event_type)
        if handler:
            return handler(self, event)
        return []

    def decide_gap_resolution(self, event: dict) -> list[Decision]:
        """Decide how to resolve capability gaps."""
        gaps = event.get("payload", {}).get("gaps", [])
        decisions = []
        for gap in gaps:
            if gap["gap_type"] == "hazmat_certification":
                candidates = gap.get("candidates", [])
                decisions.append(Decision(
                    decision_id=self._next_id(),
                    decision_type="gap_resolution",
                    action="initiate_recertification",
                    rationale=(
                        f"Hazmat certification shortfall of {gap['shortfall']}. "
                        f"{len(candidates)} candidates identified with prior certification history. "
                        f"Targeted recertification is faster than sourcing external certified workers."
                    ),
                    confidence=0.92,
                    affected_entities=candidates,
                    parameters={
                        "gap_type": "hazmat_certification",
                        "shortfall": gap["shortfall"],
                        "resolution_method": "targeted_recertification",
                    },
                ))
        return decisions

    def decide_team_assignment(self, event: dict) -> list[Decision]:
        """Decide team composition for holds."""
        teams = event.get("payload", {}).get("teams", [])
        decisions = []
        for team in teams:
            hold_id = team["hold_id"]
            decisions.append(Decision(
                decision_id=self._next_id(),
                decision_type="team_formation",
                action="assign_team",
                rationale=(
                    f"Forming team for {hold_id} based on capability matching. "
                    f"Crane operator, {len(team.get('stevedores', []))} stevedores assigned. "
                    f"{'Supervisor assigned.' if team.get('supervisor') else 'No supervisor available - escalation recommended.'}"
                ),
                confidence=0.88,
                affected_entities=[team.get("crane_operator", "")] + team.get("stevedores", []),
                parameters={"hold_id": hold_id, "team": team},
            ))
        return decisions

    def decide_degradation_response(self, event: dict) -> list[Decision]:
        """Decide response to equipment degradation."""
        payload = event.get("payload", {})
        entity_id = payload.get("entity_id", "")
        crane = self.state.cranes.get(entity_id)
        if not crane:
            return []

        degraded_rate = payload.get("degraded_moves_per_hour", 0)
        nominal_rate = crane.nominal_moves_per_hour
        hold_id = payload.get("affected_hold", crane.hold_id)

        # Find which holds have spare capacity
        other_holds = [h for h in self.state.holds.values() if h.hold_id != hold_id]
        spare_capacity = sum(
            self.state.cranes.get(h.crane_id, type("", (), {"moves_per_hour": 0, "status": EntityStatus.ACTIVE})()).moves_per_hour
            for h in other_holds
            if self.state.cranes.get(h.crane_id) and self.state.cranes[h.crane_id].status == EntityStatus.ACTIVE
        )

        decisions = [
            Decision(
                decision_id=self._next_id(),
                decision_type="degradation_response",
                action="redistribute_containers",
                rationale=(
                    f"{entity_id} degraded to {degraded_rate} moves/hr (was {nominal_rate}). "
                    f"Reduction of {nominal_rate - degraded_rate} moves/hr on {hold_id}. "
                    f"Other holds have {spare_capacity} moves/hr spare capacity. "
                    f"Redistributing containers to maintain berth throughput."
                ),
                confidence=0.95,
                affected_entities=[entity_id, hold_id],
                parameters={
                    "degraded_crane": entity_id,
                    "affected_hold": hold_id,
                    "capacity_loss": nominal_rate - degraded_rate,
                    "redistribution_strategy": "proportional",
                },
            ),
            Decision(
                decision_id=self._next_id(),
                decision_type="degradation_response",
                action="reassign_surplus_workers",
                rationale=(
                    f"With reduced throughput on {hold_id}, excess workers should be "
                    f"redistributed to holds receiving additional containers."
                ),
                confidence=0.87,
                affected_entities=[],
                parameters={"source_hold": hold_id, "strategy": "capacity_proportional"},
            ),
        ]
        return decisions

    def decide_shift_transition(self, event: dict) -> list[Decision]:
        """Decide shift transition strategy."""
        payload = event.get("payload", {})
        outgoing = payload.get("outgoing_workers", [])
        incoming = payload.get("incoming_workers", [])

        return [Decision(
            decision_id=self._next_id(),
            decision_type="shift_transition",
            action="staggered_handoff",
            rationale=(
                f"Shift change: {len(outgoing)} workers departing, {len(incoming)} arriving. "
                f"Staggered handoff: stevedores transition first, crane operators and "
                f"supervisors hand off last to maintain continuous operations. "
                f"Target: zero workforce gap on any hold."
            ),
            confidence=0.94,
            affected_entities=outgoing + incoming,
            parameters={
                "strategy": "staggered",
                "phase_1": "stevedores_transition",
                "phase_2": "new_shift_registers",
                "phase_3": "crane_ops_supervisor_handoff",
                "phase_4": "team_reformation",
                "transition_window_cycles": payload.get("transition_window_cycles", 20),
            },
        )]

    def decide_team_reformation(self, event: dict) -> list[Decision]:
        """Decide team reformation after disruption (shift change, degradation)."""
        teams = event.get("payload", {}).get("teams", [])
        decisions = []
        for team in teams:
            hold_id = team["hold_id"]
            crane_id = team.get("crane", "")
            crane = self.state.cranes.get(crane_id)
            crane_status = crane.status.value if crane else "unknown"

            decisions.append(Decision(
                decision_id=self._next_id(),
                decision_type="team_reformation",
                action="reform_team",
                rationale=(
                    f"Reforming {hold_id} team. Crane {crane_id} is {crane_status}. "
                    f"Assigning {len(team.get('stevedores', []))} stevedores, "
                    f"operator {team.get('crane_operator', 'none')}."
                ),
                confidence=0.90,
                affected_entities=team.get("stevedores", []) + [team.get("crane_operator", "")],
                parameters={"hold_id": hold_id, "team": team},
            ))
        return decisions

    def get_decisions_summary(self) -> dict:
        """Summary of all decisions made this session."""
        return {
            "total_decisions": self._decision_counter,
            "engine": "dry-run-deterministic",
        }


# Map event types to decision handlers
_EVENT_DECISION_MAP = {
    "gap_analysis": DryRunDecisionEngine.decide_gap_resolution,
    "team_formation": DryRunDecisionEngine.decide_team_assignment,
    "equipment_fault": DryRunDecisionEngine.decide_degradation_response,
    "shift_transition_start": DryRunDecisionEngine.decide_shift_transition,
    "team_reformation": DryRunDecisionEngine.decide_team_reformation,
}
