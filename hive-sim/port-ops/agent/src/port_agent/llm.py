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

    Role-aware: 'crane' processes containers, 'aggregator' produces summaries.
    """

    def __init__(self, role: str = "crane", **kwargs):
        self._cycle = 0
        self._role = role

    async def decide(self, persona: str, observed_state: dict, available_tools: list[dict]) -> AgentDecision:
        self._cycle += 1
        if self._role == "aggregator":
            return self._decide_aggregator(observed_state)
        elif self._role == "operator":
            return self._decide_operator(observed_state)
        elif self._role == "tractor":
            return self._decide_tractor(observed_state)
        elif self._role == "scheduler":
            return self._decide_scheduler(observed_state)
        elif self._role == "sensor":
            return self._decide_sensor(observed_state)
        return self._decide_crane(observed_state)

    def _decide_crane(self, observed_state: dict) -> AgentDecision:
        queue = observed_state.get("container_queue", {})
        next_containers = queue.get("next_containers", [])

        if next_containers:
            # Separate available containers into hazmat and non-hazmat
            first_hazmat = None
            first_normal = None
            for container in next_containers:
                status = container.get("status", "QUEUED")
                if status == "COMPLETED" or container.get("claimed_by"):
                    continue
                is_hazmat = container.get("hazmat", False)
                if is_hazmat and first_hazmat is None:
                    first_hazmat = container
                elif not is_hazmat and first_normal is None:
                    first_normal = container

            # Prefer processing non-hazmat; escalate hazmat on odd cycles
            if first_normal:
                cid = first_normal.get("container_id", "UNKNOWN")
                return AgentDecision(
                    action="complete_container_move",
                    arguments={"container_id": cid},
                    reasoning=f"DryRun cycle {self._cycle}: processing container {cid}",
                )

            if first_hazmat:
                cid = first_hazmat.get("container_id", "UNKNOWN")
                return AgentDecision(
                    action="request_support",
                    arguments={
                        "capability_needed": "HAZMAT_CERTIFIED_OPERATOR",
                        "reason": f"Container {cid} is hazmat class {first_hazmat.get('hazmat_class')} — need certified handler",
                    },
                    reasoning=f"DryRun cycle {self._cycle}: hazmat container {cid} needs certified handler",
                )

        return AgentDecision(
            action="wait",
            arguments={"reason": "Queue empty — all containers processed"},
            reasoning=f"DryRun cycle {self._cycle}: no containers remaining",
        )

    def _decide_operator(self, observed_state: dict) -> AgentDecision:
        tasking = observed_state.get("tasking", {})
        status = tasking.get("status", "AVAILABLE")
        assigned_to = tasking.get("assigned_to")
        pending = tasking.get("pending_requests", [])

        # Cycle 12+: simulate shift change
        if self._cycle >= 12:
            return AgentDecision(
                action="report_available",
                arguments={"status": "OFF_SHIFT", "details": "End of shift"},
                reasoning=f"DryRun cycle {self._cycle}: shift ending",
            )

        # Every 8th cycle: simulate break
        if self._cycle % 8 == 0 and status != "BREAK":
            return AgentDecision(
                action="report_available",
                arguments={"status": "BREAK", "details": "Taking 15-min break"},
                reasoning=f"DryRun cycle {self._cycle}: scheduled break",
            )

        # If on break or off shift, come back available
        if status in ("BREAK", "OFF_SHIFT") and self._cycle < 12:
            return AgentDecision(
                action="report_available",
                arguments={"status": "AVAILABLE", "details": "Returning from break"},
                reasoning=f"DryRun cycle {self._cycle}: returning to available",
            )

        # If assigned to a crane, complete the assignment (crane has had a cycle to move)
        if assigned_to is not None:
            return AgentDecision(
                action="complete_assignment",
                arguments={},
                reasoning=f"DryRun cycle {self._cycle}: completing assignment to {assigned_to}",
            )

        # If not available yet, check in
        if status not in ("AVAILABLE",):
            return AgentDecision(
                action="report_available",
                arguments={"status": "AVAILABLE", "details": "Checking in for shift"},
                reasoning=f"DryRun cycle {self._cycle}: checking in",
            )

        # If pending crane requests, accept first one
        if pending:
            crane_id = pending[0]
            return AgentDecision(
                action="accept_assignment",
                arguments={"crane_id": crane_id},
                reasoning=f"DryRun cycle {self._cycle}: accepting assignment to {crane_id}",
            )

        return AgentDecision(
            action="wait",
            arguments={"reason": "Available — no pending crane requests"},
            reasoning=f"DryRun cycle {self._cycle}: operator idle, waiting for assignment",
        )

    def _decide_tractor(self, observed_state: dict) -> AgentDecision:
        tasking = observed_state.get("tasking", {})
        battery = tasking.get("battery_pct", 100)
        pending_jobs = tasking.get("pending_transport_jobs", [])
        trips = tasking.get("trips_completed", 0)

        # Battery critical: request charge
        if battery < 30:
            return AgentDecision(
                action="request_charge",
                arguments={},
                reasoning=f"DryRun cycle {self._cycle}: battery at {battery}%, requesting charge",
            )

        # Cycle 14+: return to depot
        if self._cycle >= 14:
            return AgentDecision(
                action="report_position",
                arguments={"zone": "yard", "block": "DEPOT", "status": "IDLE"},
                reasoning=f"DryRun cycle {self._cycle}: returning to depot",
            )

        # Every 6th cycle: report position
        if self._cycle % 6 == 0:
            return AgentDecision(
                action="report_position",
                arguments={"zone": "yard", "block": f"YB-{chr(65 + (trips % 6))}", "status": "IN_TRANSIT"},
                reasoning=f"DryRun cycle {self._cycle}: periodic position report",
            )

        # Primary: transport container from queue
        if pending_jobs:
            job = pending_jobs[0]
            return AgentDecision(
                action="transport_container",
                arguments={
                    "container_id": job.get("container_id", "UNKNOWN"),
                    "destination_block": job.get("destination_block", "YB-A01"),
                },
                reasoning=f"DryRun cycle {self._cycle}: transporting {job.get('container_id')}",
            )

        return AgentDecision(
            action="wait",
            arguments={"reason": "No transport jobs pending"},
            reasoning=f"DryRun cycle {self._cycle}: tractor idle, no jobs",
        )

    def _decide_scheduler(self, observed_state: dict) -> AgentDecision:
        team = observed_state.get("team_state", {})
        members = team.get("team_members", {})
        gaps = team.get("gap_analysis", [])

        # If crane DEGRADED: dispatch resource
        for mid, mdata in members.items():
            if mdata.get("entity_type") == "gantry_crane" and mdata.get("status") == "DEGRADED":
                return AgentDecision(
                    action="dispatch_resource",
                    arguments={
                        "resource_type": "tractor",
                        "from_entity": "tractor-1",
                        "to_entity": mid,
                        "reason": f"{mid} degraded — rerouting transport support",
                    },
                    reasoning=f"DryRun cycle {self._cycle}: dispatching to degraded crane {mid}",
                )

        # Every 2nd cycle: rebalance assignments
        if self._cycle % 2 == 0:
            operators = [m for m, d in members.items() if d.get("entity_type") == "operator"]
            cranes = [m for m, d in members.items() if d.get("entity_type") == "gantry_crane"]
            assignments = {}
            for i, op in enumerate(operators[:len(cranes)]):
                assignments[op] = cranes[i % len(cranes)] if cranes else "crane-1"
            return AgentDecision(
                action="rebalance_assignments",
                arguments={"assignments": assignments},
                reasoning=f"DryRun cycle {self._cycle}: periodic rebalance",
            )

        # Every 4th cycle: update priority queue
        if self._cycle % 4 == 0:
            return AgentDecision(
                action="update_priority_queue",
                arguments={"priority_order": ["MSCU-4472891", "MSCU-4472892", "MSCU-4472893"]},
                reasoning=f"DryRun cycle {self._cycle}: reprioritizing queue",
            )

        # If gaps detected: dispatch resource
        if gaps:
            return AgentDecision(
                action="dispatch_resource",
                arguments={
                    "resource_type": "operator",
                    "from_entity": "op-3",
                    "to_entity": gaps[-1].get("reported_by", "crane-1"),
                    "reason": f"Gap: {gaps[-1].get('capability', 'unknown')}",
                },
                reasoning=f"DryRun cycle {self._cycle}: dispatching for gap",
            )

        return AgentDecision(
            action="emit_schedule_event",
            arguments={
                "event_type": "schedule_check",
                "details": f"Cycle {self._cycle}: all systems nominal",
                "priority": "LOW",
            },
            reasoning=f"DryRun cycle {self._cycle}: routine schedule check",
        )

    def _decide_sensor(self, observed_state: dict) -> AgentDecision:
        tasking = observed_state.get("tasking", {})
        sensor_type = tasking.get("sensor_type", "LOAD_CELL")

        # Every 5th cycle: report calibration
        if self._cycle % 5 == 0:
            # Simulate drift increasing over time
            drift = self._cycle * 0.3
            accuracy = max(75.0, 100.0 - drift)
            status = "CALIBRATED" if accuracy >= 95 else "DRIFTING" if accuracy >= 85 else "NEEDS_RECALIBRATION"
            return AgentDecision(
                action="report_calibration",
                arguments={
                    "accuracy_pct": round(accuracy, 1),
                    "drift": round(drift, 2),
                    "status": status,
                },
                reasoning=f"DryRun cycle {self._cycle}: calibration report (accuracy={accuracy:.1f}%)",
            )

        # All other cycles: emit readings (sensors never wait)
        if sensor_type == "LOAD_CELL":
            # Weight reading — slight variation, occasional anomaly
            base_weight = 25.0
            variation = (self._cycle % 7) * 0.5
            value = base_weight + variation
            if self._cycle % 11 == 0:
                value = base_weight * 1.08  # Anomaly: >5% divergence
            return AgentDecision(
                action="emit_reading",
                arguments={
                    "reading_type": "weight",
                    "value": round(value, 1),
                    "unit": "tons",
                    "container_id": f"MSCU-{4472891 + (self._cycle % 20)}",
                },
                reasoning=f"DryRun cycle {self._cycle}: load cell reading {value:.1f}t",
            )
        else:
            # RFID tag scan
            return AgentDecision(
                action="emit_reading",
                arguments={
                    "reading_type": "rfid_tag",
                    "value": 4472891 + (self._cycle % 20),
                    "unit": "id",
                    "container_id": f"MSCU-{4472891 + (self._cycle % 20)}",
                },
                reasoning=f"DryRun cycle {self._cycle}: RFID scan",
            )

    def _decide_aggregator(self, observed_state: dict) -> AgentDecision:
        team = observed_state.get("team_state", {})
        tasking = observed_state.get("tasking", {})

        # Every 3rd cycle: produce a hold summary
        if self._cycle % 3 == 0:
            moves_completed = team.get("moves_completed", 0)
            target = team.get("target_moves_per_hour", 35)
            members = team.get("team_members", {})
            crane_count = sum(
                1 for m in members.values()
                if m.get("entity_type") == "gantry_crane"
            )
            # Estimate rate: moves_completed * (60 / simulated_minutes_elapsed)
            # For dry-run, use a simple heuristic
            estimated_rate = crane_count * 15  # rough estimate per crane
            status = "ACTIVE" if estimated_rate >= target * 0.8 else "DEGRADED"

            return AgentDecision(
                action="update_hold_summary",
                arguments={
                    "moves_per_hour": estimated_rate,
                    "status": status,
                    "summary": (
                        f"{crane_count} cranes active, {moves_completed} moves completed, "
                        f"est. {estimated_rate} moves/hr"
                    ),
                },
                reasoning=(
                    f"DryRun cycle {self._cycle}: periodic hold summary — "
                    f"{crane_count} cranes, {moves_completed} moves"
                ),
            )

        # If gaps exist, emit a hold event
        gaps = team.get("gap_analysis", [])
        if gaps:
            latest_gap = gaps[-1]
            return AgentDecision(
                action="emit_hold_event",
                arguments={
                    "event_type": "gap_detected",
                    "details": (
                        f"Capability gap: {latest_gap.get('capability', 'unknown')} "
                        f"reported by {latest_gap.get('reported_by', 'unknown')}"
                    ),
                    "priority": "HIGH",
                },
                reasoning=(
                    f"DryRun cycle {self._cycle}: {len(gaps)} capability gap(s) detected"
                ),
            )

        return AgentDecision(
            action="wait",
            arguments={"reason": "Monitoring — no summary due, no gaps detected"},
            reasoning=f"DryRun cycle {self._cycle}: aggregator idle, waiting for data",
        )


def create_provider(provider_type: str = "anthropic", **kwargs) -> LLMProvider:
    """Factory for LLM providers."""
    # DryRunProvider accepts 'role'; other providers don't — pop it for safety
    role = kwargs.pop("role", "crane")
    if provider_type == "anthropic":
        return AnthropicProvider(**kwargs)
    elif provider_type in ("ollama", "openai"):
        return OllamaProvider(**kwargs)
    elif provider_type == "dry-run":
        return DryRunProvider(role=role, **kwargs)
    else:
        raise ValueError(f"Unknown provider: {provider_type}. Use 'anthropic', 'ollama', or 'dry-run'.")
