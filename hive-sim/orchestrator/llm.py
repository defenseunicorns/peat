"""
LLM provider abstraction for simulation equipment agents.

Supports tiered LLM runtime per Addendum A §LLM Runtime Options (Option C: Tiered):
- dry-run: No LLM, returns canned responses (for testing)
- local_slm: Local small model via Ollama (for equipment agents)
- api: Remote API provider (for sophisticated reasoning)

Equipment agents (cranes, tractors) use local_slm for low-latency,
no-external-dependency reasoning suitable for lift sequencing and route decisions.
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
