"""
LLM Provider Abstraction

Supports multiple backends for agent reasoning:
- API mode: Claude (Anthropic) or OpenAI-compatible endpoints
- Local mode: Ollama or any OpenAI-compatible local server

The agent loop calls `decide()` with observed state and gets back
a structured action decision.
"""

from __future__ import annotations

import json
import logging
import os
from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Any

logger = logging.getLogger(__name__)


@dataclass
class AgentDecision:
    """Structured output from LLM reasoning."""
    action: str                    # Tool name to call (or "wait")
    arguments: dict[str, Any]      # Tool arguments
    reasoning: str                 # Brief explanation of why
    confidence: float = 1.0        # 0.0-1.0


class LLMProvider(ABC):
    """Abstract LLM provider interface."""

    @abstractmethod
    async def decide(
        self,
        persona: str,
        observed_state: dict[str, Any],
        available_tools: list[dict],
    ) -> AgentDecision:
        """
        Given a persona and observed state, decide what action to take.

        Args:
            persona: The agent's persona/system prompt
            observed_state: Current HIVE state from MCP resources
            available_tools: List of MCP tool schemas

        Returns:
            AgentDecision with action, arguments, and reasoning
        """
        ...


class AnthropicProvider(LLMProvider):
    """Claude API provider."""

    def __init__(self, model: str = "claude-sonnet-4-5-20250929", api_key: str | None = None):
        try:
            import anthropic
        except ImportError:
            raise ImportError("Install anthropic: pip install anthropic")

        self.model = model
        self.client = anthropic.AsyncAnthropic(
            api_key=api_key or os.environ.get("ANTHROPIC_API_KEY"),
        )

    async def decide(self, persona: str, observed_state: dict, available_tools: list[dict]) -> AgentDecision:
        # Build tool definitions for Claude
        tools = []
        for tool in available_tools:
            tools.append({
                "name": tool["name"],
                "description": tool.get("description", ""),
                "input_schema": tool.get("inputSchema", {"type": "object", "properties": {}}),
            })

        # Add "wait" as a pseudo-tool
        tools.append({
            "name": "wait",
            "description": "Do nothing this cycle. Use when no action is needed (e.g., waiting for operator, no containers queued).",
            "input_schema": {
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "Why you are waiting",
                    },
                },
                "required": ["reason"],
            },
        })

        state_text = json.dumps(observed_state, indent=2, default=str)

        message = await self.client.messages.create(
            model=self.model,
            max_tokens=1024,
            system=persona,
            tools=tools,
            messages=[
                {
                    "role": "user",
                    "content": (
                        f"OBSERVE — Current HIVE state:\n\n```json\n{state_text}\n```\n\n"
                        "ORIENT and DECIDE — Based on your persona, the current state, "
                        "and your constraints, what is your next action? "
                        "You MUST call exactly one tool."
                    ),
                },
            ],
        )

        # Extract tool use from response
        for block in message.content:
            if block.type == "tool_use":
                reasoning = ""
                # Look for text block before tool use for reasoning
                for b in message.content:
                    if b.type == "text":
                        reasoning = b.text
                        break

                return AgentDecision(
                    action=block.name,
                    arguments=block.input,
                    reasoning=reasoning,
                )

        # Fallback if no tool was called
        text = "".join(b.text for b in message.content if b.type == "text")
        return AgentDecision(
            action="wait",
            arguments={"reason": text or "No action determined"},
            reasoning=text,
            confidence=0.5,
        )


class OllamaProvider(LLMProvider):
    """Ollama (or any OpenAI-compatible) local provider."""

    def __init__(
        self,
        model: str = "qwen3:1.7b",
        base_url: str = "http://localhost:11434/v1",
        api_key: str = "ollama",
    ):
        try:
            import openai
        except ImportError:
            raise ImportError("Install openai: pip install openai")

        self.model = model
        self.client = openai.AsyncOpenAI(base_url=base_url, api_key=api_key)

    async def decide(self, persona: str, observed_state: dict, available_tools: list[dict]) -> AgentDecision:
        # Build OpenAI-format tools
        tools = []
        for tool in available_tools:
            tools.append({
                "type": "function",
                "function": {
                    "name": tool["name"],
                    "description": tool.get("description", ""),
                    "parameters": tool.get("inputSchema", {"type": "object", "properties": {}}),
                },
            })

        # Add wait tool
        tools.append({
            "type": "function",
            "function": {
                "name": "wait",
                "description": "Do nothing this cycle.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "reason": {"type": "string", "description": "Why you are waiting"},
                    },
                    "required": ["reason"],
                },
            },
        })

        state_text = json.dumps(observed_state, indent=2, default=str)

        response = await self.client.chat.completions.create(
            model=self.model,
            messages=[
                {"role": "system", "content": persona},
                {
                    "role": "user",
                    "content": (
                        f"OBSERVE — Current HIVE state:\n\n```json\n{state_text}\n```\n\n"
                        "ORIENT and DECIDE — Based on your persona, the current state, "
                        "and your constraints, what is your next action? "
                        "You MUST call exactly one tool."
                    ),
                },
            ],
            tools=tools,
            tool_choice="required",
            max_tokens=1024,
        )

        choice = response.choices[0]

        if choice.message.tool_calls:
            tc = choice.message.tool_calls[0]
            try:
                args = json.loads(tc.function.arguments)
            except json.JSONDecodeError:
                args = {"raw": tc.function.arguments}

            return AgentDecision(
                action=tc.function.name,
                arguments=args,
                reasoning=choice.message.content or "",
            )

        return AgentDecision(
            action="wait",
            arguments={"reason": choice.message.content or "No tool call"},
            reasoning=choice.message.content or "",
            confidence=0.5,
        )


class DryRunProvider(LLMProvider):
    """Deterministic provider for testing without an LLM API.

    Cycles through the container queue calling complete_container_move,
    making it possible to validate the full OODA loop offline.
    """

    def __init__(self, **kwargs):
        self._cycle = 0

    async def decide(self, persona: str, observed_state: dict, available_tools: list[dict]) -> AgentDecision:
        self._cycle += 1
        queue = observed_state.get("container_queue", {})
        next_containers = queue.get("next_containers", [])

        if next_containers:
            # Find first non-hazmat container, or escalate the first hazmat
            for container in next_containers:
                cid = container.get("container_id", "UNKNOWN")
                is_hazmat = container.get("hazmat", False)
                status = container.get("status", "QUEUED")

                if status == "COMPLETED":
                    continue

                if is_hazmat:
                    return AgentDecision(
                        action="request_support",
                        arguments={
                            "capability_needed": "HAZMAT_CERTIFIED_OPERATOR",
                            "reason": f"Container {cid} is hazmat class {container.get('hazmat_class')} — need certified handler",
                        },
                        reasoning=f"DryRun cycle {self._cycle}: hazmat container {cid} needs certified handler",
                    )

                return AgentDecision(
                    action="complete_container_move",
                    arguments={"container_id": cid},
                    reasoning=f"DryRun cycle {self._cycle}: processing container {cid}",
                )

        return AgentDecision(
            action="wait",
            arguments={"reason": "Queue empty — all containers processed"},
            reasoning=f"DryRun cycle {self._cycle}: no containers remaining",
        )


def create_provider(provider_type: str = "anthropic", **kwargs) -> LLMProvider:
    """Factory for LLM providers."""
    if provider_type == "anthropic":
        return AnthropicProvider(**kwargs)
    elif provider_type in ("ollama", "openai"):
        return OllamaProvider(**kwargs)
    elif provider_type == "dry-run":
        return DryRunProvider(**kwargs)
    else:
        raise ValueError(f"Unknown provider: {provider_type}. Use 'anthropic', 'ollama', or 'dry-run'.")
