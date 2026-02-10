"""
LLM Provider Abstraction

Supports multiple backends for agent reasoning:
- API mode: Claude (Anthropic) or OpenAI-compatible endpoints
- Local mode: Ollama or any OpenAI-compatible local server

The agent loop calls `decide()` with observed state and gets back
a structured action decision.
"""

from __future__ import annotations

import asyncio
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

    # Simulate LLM thinking time so the viewer can keep up
    DRY_RUN_DELAY_S = 0.3

    def __init__(self, role: str = "crane", **kwargs):
        self._cycle = 0
        self._role = role
        self._proficiency = kwargs.get("proficiency", "competent")
        self._BASE_DELAY_S = self.DRY_RUN_DELAY_S
        self._speed_factor = 1.0
        self._error_rate = 0.0

    async def decide(self, persona: str, observed_state: dict, available_tools: list[dict]) -> AgentDecision:
        self._cycle += 1
        # Proficiency-scaled thinking delay
        await asyncio.sleep(self._BASE_DELAY_S * self._speed_factor)

        # Proficiency-based error: occasionally make wrong tool call
        import random
        if self._error_rate > 0 and random.random() < self._error_rate:
            return AgentDecision(
                action="wait",
                arguments={"reason": f"Hesitation — {self._proficiency} re-evaluating situation"},
                reasoning=(
                    f"DryRun cycle {self._cycle}: proficiency error "
                    f"({self._proficiency}, {self._error_rate:.0%} rate) — delayed response"
                ),
                confidence=0.5,
            )

        # ── Labor constraint checks (applies to worker roles) ────────────
        labor = observed_state.get("labor_constraints", {})
        if labor:
            # Shift expired → worker goes off shift
            if labor.get("shift_ended"):
                return AgentDecision(
                    action="report_available" if self._role == "operator" else "wait",
                    arguments=(
                        {"status": "OFF_SHIFT", "details": "Shift ended per ILA Local 1414 rules"}
                        if self._role == "operator"
                        else {"reason": "Shift ended — off duty per union contract"}
                    ),
                    reasoning=(
                        f"DryRun cycle {self._cycle}: shift expired "
                        f"({labor.get('shift_elapsed_hours', 0):.1f}h worked), going off shift"
                    ),
                )

            # Mandatory break required → worker takes break
            if labor.get("break_required"):
                return AgentDecision(
                    action="report_available" if self._role == "operator" else "wait",
                    arguments=(
                        {"status": "BREAK", "details": f"Mandatory break — {labor.get('consecutive_hours', 0):.1f}h consecutive work"}
                        if self._role == "operator"
                        else {"reason": f"Mandatory break — {labor.get('consecutive_hours', 0):.1f}h consecutive (max {labor.get('max_consecutive_hours', 6)}h)"}
                    ),
                    reasoning=(
                        f"DryRun cycle {self._cycle}: mandatory break required "
                        f"({labor.get('consecutive_hours', 0):.1f}h consecutive, "
                        f"max {labor.get('max_consecutive_hours', 6)}h)"
                    ),
                )

            # On break → wait for break to complete
            if labor.get("on_break"):
                return AgentDecision(
                    action="wait",
                    arguments={"reason": f"On break ({labor.get('break_duration_min', 30)} min mandatory)"},
                    reasoning=f"DryRun cycle {self._cycle}: on mandatory break",
                )

            # Crane: crew insufficient → pause operations
            if self._role == "crane" and labor.get("crew_insufficient"):
                return AgentDecision(
                    action="request_support",
                    arguments={
                        "capability_needed": "SIGNALER",
                        "reason": (
                            f"Crew below minimum ({labor.get('current_crew', 0)}"
                            f"/{labor.get('minimum_crew', 2)}) — "
                            f"ILA Local 1414 requires operator + signaler"
                        ),
                    },
                    reasoning=(
                        f"DryRun cycle {self._cycle}: crane paused — "
                        f"crew {labor.get('current_crew', 0)}/{labor.get('minimum_crew', 2)} "
                        f"below ILA minimum"
                    ),
                )
        if self._role == "aggregator":
            return self._decide_aggregator(observed_state)
        elif self._role == "berth_manager":
            return self._decide_berth_manager(observed_state)
        elif self._role == "operator":
            return self._decide_operator(observed_state)
        elif self._role == "tractor":
            return self._decide_tractor(observed_state)
        elif self._role == "scheduler":
            return self._decide_scheduler(observed_state)
        elif self._role == "sensor":
            return self._decide_sensor(observed_state)
        elif self._role == "lashing_crew":
            return self._decide_lashing_crew(observed_state)
        elif self._role == "signaler":
            return self._decide_signaler(observed_state)
        elif self._role == "gate_manager":
            return self._decide_gate_manager(observed_state)
        elif self._role == "gate_scanner":
            return self._decide_gate_scanner(observed_state)
        elif self._role == "rfid_reader":
            return self._decide_rfid_reader(observed_state)
        elif self._role == "gate_worker":
            return self._decide_gate_worker(observed_state)
        return self._decide_crane(observed_state)

    def _decide_crane(self, observed_state: dict) -> AgentDecision:
        tasking = observed_state.get("tasking", {})
        queue = observed_state.get("container_queue", {})
        next_containers = queue.get("next_containers", [])
        # Phase 2: prefer containers specifically assigned to this crane
        assigned_containers = tasking.get("assigned_containers", [])

        if assigned_containers:
            # Process next assigned container that's still in the queue
            for cid in assigned_containers:
                target = next(
                    (c for c in next_containers
                     if c.get("container_id") == cid
                     and c.get("status") != "COMPLETED"
                     and not c.get("claimed_by")),
                    None,
                )
                if target:
                    if target.get("hazmat"):
                        return AgentDecision(
                            action="request_support",
                            arguments={
                                "capability_needed": "HAZMAT_CERTIFIED_OPERATOR",
                                "reason": f"Assigned container {cid} is hazmat class {target.get('hazmat_class')} — need certified handler",
                            },
                            reasoning=f"DryRun cycle {self._cycle}: assigned hazmat container {cid}",
                        )
                    return AgentDecision(
                        action="complete_container_move",
                        arguments={"container_id": cid},
                        reasoning=f"DryRun cycle {self._cycle}: processing assigned container {cid}",
                    )

        # Fallback: process next available container from queue (backward compat)
        if next_containers:
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

        # Cycle 40+: simulate shift change (ADR-030 T+40)
        if self._cycle >= 40:
            return AgentDecision(
                action="report_available",
                arguments={"status": "OFF_SHIFT", "details": "End of shift"},
                reasoning=f"DryRun cycle {self._cycle}: shift ending",
            )

        # Every 15th cycle: simulate break
        if self._cycle % 15 == 0 and status != "BREAK":
            return AgentDecision(
                action="report_available",
                arguments={"status": "BREAK", "details": "Taking 15-min break"},
                reasoning=f"DryRun cycle {self._cycle}: scheduled break",
            )

        # If on break or off shift, come back available
        if status in ("BREAK", "OFF_SHIFT") and self._cycle < 40:
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
        # Phase 2: containers specifically assigned to this tractor
        assigned_containers = tasking.get("assigned_containers", [])

        # Battery critical: request charge
        if battery < 30:
            return AgentDecision(
                action="request_charge",
                arguments={},
                reasoning=f"DryRun cycle {self._cycle}: battery at {battery}%, requesting charge",
            )

        # Cycle 42+: return to depot (after shift change)
        if self._cycle >= 42:
            return AgentDecision(
                action="report_position",
                arguments={"zone": "yard", "block": "DEPOT", "status": "IDLE"},
                reasoning=f"DryRun cycle {self._cycle}: returning to depot",
            )

        # Every 10th cycle: report position
        if self._cycle % 10 == 0:
            return AgentDecision(
                action="report_position",
                arguments={"zone": "yard", "block": f"YB-{chr(65 + (trips % 6))}", "status": "IN_TRANSIT"},
                reasoning=f"DryRun cycle {self._cycle}: periodic position report",
            )

        # Phase 2: prefer assigned containers from transport queue
        if assigned_containers:
            for cid in assigned_containers:
                job = next(
                    (j for j in pending_jobs if j.get("container_id") == cid),
                    None,
                )
                if job:
                    return AgentDecision(
                        action="transport_container",
                        arguments={
                            "container_id": cid,
                            "destination_block": job.get("destination_block", "YB-A01"),
                        },
                        reasoning=f"DryRun cycle {self._cycle}: transporting assigned container {cid}",
                    )

        # Fallback: transport first available from queue (backward compat)
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
        tasking = observed_state.get("tasking", {})
        members = team.get("team_members", {})
        gaps = team.get("gap_analysis", [])
        # Phase 2: container-level assignment
        unassigned = tasking.get("unassigned_containers", [])

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

        # Phase 2: assign unassigned containers to specific cranes/operators/tractors
        if unassigned:
            cranes = sorted(
                [m for m, d in members.items() if d.get("entity_type") == "gantry_crane"],
            )
            operators = sorted(
                [m for m, d in members.items() if d.get("entity_type") == "operator"],
            )
            tractors = sorted(
                [m for m, d in members.items() if d.get("entity_type") == "yard_tractor"],
            )

            # Round-robin assign next unassigned container
            cid = unassigned[0]
            crane_idx = self._cycle % len(cranes) if cranes else 0
            op_idx = self._cycle % len(operators) if operators else 0
            tractor_idx = self._cycle % len(tractors) if tractors else 0

            args = {
                "container_id": cid,
                "assigned_crane": cranes[crane_idx] if cranes else "crane-1",
            }
            if operators:
                args["assigned_operator"] = operators[op_idx]
            if tractors:
                args["assigned_tractor"] = tractors[tractor_idx]

            return AgentDecision(
                action="assign_container",
                arguments=args,
                reasoning=(
                    f"DryRun cycle {self._cycle}: assigning container {cid} to "
                    f"{args.get('assigned_crane')}"
                ),
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

    def _decide_berth_manager(self, observed_state: dict) -> AgentDecision:
        tasking = observed_state.get("tasking", {})
        hold_summaries = tasking.get("hold_summaries", [])
        total_remaining = tasking.get("total_moves_remaining", 0)

        # Every 5 cycles: produce a berth summary
        if self._cycle % 5 == 0:
            total_rate = sum(h.get("moves_per_hour", 0) for h in hold_summaries)
            # Estimate ETA: remaining moves / rate (in minutes)
            eta_minutes = round((total_remaining / max(total_rate, 1)) * 60, 1) if total_remaining > 0 else 0.0
            hold_statuses = {
                h.get("hold_id", f"hold-{i}"): h.get("status", "UNKNOWN")
                for i, h in enumerate(hold_summaries)
            }
            return AgentDecision(
                action="update_berth_summary",
                arguments={
                    "total_moves_per_hour": total_rate,
                    "hold_statuses": hold_statuses,
                    "completion_eta_minutes": eta_minutes,
                    "summary": (
                        f"{len(hold_summaries)} holds reporting, "
                        f"{total_rate} moves/hr aggregate, "
                        f"ETA {eta_minutes} min"
                    ),
                },
                reasoning=(
                    f"DryRun cycle {self._cycle}: periodic berth summary — "
                    f"{total_rate} moves/hr, {total_remaining} remaining"
                ),
            )

        # Cross-hold coordination: detect imbalance and take corrective action
        if len(hold_summaries) > 1:
            rates = [h.get("moves_per_hour", 0) for h in hold_summaries]
            avg_rate = sum(rates) / len(rates) if rates else 0

            for h in hold_summaries:
                h_rate = h.get("moves_per_hour", 0)
                hold_id = h.get("hold_id", "?")
                target_rate = h.get("target_rate", 35)

                # Check if hold is below 80% of target rate
                if target_rate > 0 and h_rate < target_rate * 0.8:
                    degraded = h.get("degraded_equipment", {})
                    gap_count = h.get("gap_count", 0)
                    worker_counts = h.get("worker_counts", {})

                    # Analyze cause: equipment degradation
                    if degraded:
                        failed_ids = [
                            mid for mid, st in degraded.items()
                            if st == "FAILED"
                        ]
                        if failed_ids:
                            # Structural problem — escalate to scheduler
                            return AgentDecision(
                                action="escalate_to_scheduler",
                                arguments={
                                    "issue_type": "crane_failure",
                                    "details": (
                                        f"Hold {hold_id}: equipment failure on "
                                        f"{', '.join(failed_ids)}. "
                                        f"Rate {h_rate} vs target {target_rate} moves/hr. "
                                        f"May require stow plan change."
                                    ),
                                },
                                reasoning=(
                                    f"DryRun cycle {self._cycle}: crane failure in {hold_id}, "
                                    f"escalating to scheduler"
                                ),
                            )

                        # Non-fatal degradation — raise hold priority
                        return AgentDecision(
                            action="update_hold_priority",
                            arguments={
                                "hold_num": int(hold_id.split("-")[-1]) if "-" in hold_id else 1,
                                "priority_level": "HIGH",
                                "reason": (
                                    f"Equipment degradation on {', '.join(degraded.keys())} — "
                                    f"rate {h_rate} below target {target_rate}"
                                ),
                            },
                            reasoning=(
                                f"DryRun cycle {self._cycle}: degraded equipment in {hold_id}, "
                                f"raising priority"
                            ),
                        )

                    # Analyze cause: workforce gap
                    operator_count = worker_counts.get("operator", 0)
                    crane_count = worker_counts.get("gantry_crane", 0)
                    if crane_count > 0 and operator_count < crane_count:
                        # Find a hold with surplus workers to reassign
                        best_source = self._find_surplus_hold(
                            hold_summaries, hold_id, "operator"
                        )
                        if best_source:
                            return AgentDecision(
                                action="reassign_worker",
                                arguments={
                                    "worker_id": f"op-{self._cycle % 10}",
                                    "from_hold": best_source,
                                    "to_hold": hold_id,
                                    "reason": (
                                        f"{hold_id} has {operator_count} operators for "
                                        f"{crane_count} cranes — below ratio"
                                    ),
                                },
                                reasoning=(
                                    f"DryRun cycle {self._cycle}: workforce gap in {hold_id}, "
                                    f"reassigning from {best_source}"
                                ),
                            )

                    # Analyze cause: tractor shortage — dispatch shared tractor
                    tractor_count = worker_counts.get("yard_tractor", 0)
                    if tractor_count < 2 and h.get("moves_remaining", 0) > 0:
                        best_source = self._find_surplus_hold(
                            hold_summaries, hold_id, "yard_tractor"
                        )
                        if best_source:
                            return AgentDecision(
                                action="request_tractor_rebalance",
                                arguments={
                                    "from_hold": best_source,
                                    "to_hold": hold_id,
                                    "reason": (
                                        f"{hold_id} has {tractor_count} tractor(s) and is "
                                        f"at {h_rate} moves/hr (target {target_rate})"
                                    ),
                                },
                                reasoning=(
                                    f"DryRun cycle {self._cycle}: tractor shortage in {hold_id}, "
                                    f"rebalancing from {best_source}"
                                ),
                            )

                    # Generic cross-hold gap — no specific cause identified
                    if avg_rate > 0 and h_rate < avg_rate * 0.8:
                        return AgentDecision(
                            action="emit_berth_event",
                            arguments={
                                "event_type": "cross_hold_gap",
                                "details": (
                                    f"Hold {hold_id} at {h_rate} moves/hr "
                                    f"vs avg {avg_rate:.0f} — below 80% threshold"
                                ),
                                "priority": "HIGH",
                            },
                            reasoning=(
                                f"DryRun cycle {self._cycle}: cross-hold gap detected in {hold_id}"
                            ),
                        )

        # Persistent gap escalation: if a hold has gaps for 3+ consecutive checks
        for h in hold_summaries:
            gap_count = h.get("gap_count", 0)
            if gap_count >= 3:
                return AgentDecision(
                    action="escalate_to_scheduler",
                    arguments={
                        "issue_type": "persistent_throughput_gap",
                        "details": (
                            f"Hold {h.get('hold_id', '?')} has "
                            f"{gap_count} persistent unresolved gaps — "
                            f"berth-level rebalancing insufficient"
                        ),
                    },
                    reasoning=(
                        f"DryRun cycle {self._cycle}: persistent gaps in "
                        f"{h.get('hold_id', '?')}, escalating to scheduler"
                    ),
                )

        # Escalate remaining hold gaps
        for h in hold_summaries:
            if h.get("gap_count", 0) > 0:
                return AgentDecision(
                    action="emit_berth_event",
                    arguments={
                        "event_type": "hold_gap_escalation",
                        "details": (
                            f"Hold {h.get('hold_id', '?')} has "
                            f"{h.get('gap_count', 0)} unresolved gaps"
                        ),
                        "priority": "HIGH",
                    },
                    reasoning=(
                        f"DryRun cycle {self._cycle}: escalating hold gaps to berth level"
                    ),
                )

        # Every 7 cycles: request tractor rebalance if work remains
        if self._cycle % 7 == 0 and total_remaining > 0:
            # Find actual imbalance rather than hardcoded holds
            if len(hold_summaries) >= 2:
                sorted_holds = sorted(
                    hold_summaries, key=lambda h: h.get("moves_per_hour", 0)
                )
                slowest = sorted_holds[0]
                fastest = sorted_holds[-1]
                return AgentDecision(
                    action="request_tractor_rebalance",
                    arguments={
                        "from_hold": fastest.get("hold_id", "hold-1"),
                        "to_hold": slowest.get("hold_id", "hold-3"),
                        "reason": (
                            f"Periodic rebalance — {slowest.get('hold_id')} at "
                            f"{slowest.get('moves_per_hour', 0)} moves/hr vs "
                            f"{fastest.get('hold_id')} at "
                            f"{fastest.get('moves_per_hour', 0)} moves/hr"
                        ),
                    },
                    reasoning=(
                        f"DryRun cycle {self._cycle}: periodic tractor rebalance"
                    ),
                )
            return AgentDecision(
                action="request_tractor_rebalance",
                arguments={
                    "from_hold": "hold-1",
                    "to_hold": "hold-3",
                    "reason": "Periodic rebalance — evening out tractor distribution",
                },
                reasoning=(
                    f"DryRun cycle {self._cycle}: periodic tractor rebalance"
                ),
            )

        return AgentDecision(
            action="wait",
            arguments={"reason": "Monitoring — no summary due, no cross-hold gaps"},
            reasoning=f"DryRun cycle {self._cycle}: berth manager idle, waiting for data",
        )

    @staticmethod
    def _find_surplus_hold(
        hold_summaries: list[dict], exclude_hold_id: str, entity_type: str
    ) -> str | None:
        """Find a hold with surplus workers/tractors that can donate to another.

        Returns the hold_id of the best donor, or None if no surplus found.
        """
        best_hold = None
        best_surplus = 0
        for h in hold_summaries:
            hid = h.get("hold_id", "")
            if hid == exclude_hold_id:
                continue
            count = h.get("worker_counts", {}).get(entity_type, 0)
            # Consider a hold a surplus source if it has more than 1 of this type
            if count > 1 and count > best_surplus:
                best_surplus = count
                best_hold = hid
        return best_hold

    def _decide_lashing_crew(self, observed_state: dict) -> AgentDecision:
        tasking = observed_state.get("tasking", {})
        containers_secured = tasking.get("containers_secured", 0)
        pending_containers = tasking.get("pending_containers", [])

        # Every 6th cycle: inspect existing lashings
        if self._cycle % 6 == 0:
            return AgentDecision(
                action="inspect_lashing",
                arguments={
                    "container_id": f"MSCU-{4472891 + (containers_secured % 20)}",
                    "result": "PASS",
                },
                reasoning=f"DryRun cycle {self._cycle}: periodic lashing inspection",
            )

        # Every 10th cycle: request lashing tools
        if self._cycle % 10 == 0:
            return AgentDecision(
                action="request_lashing_tools",
                arguments={},
                reasoning=f"DryRun cycle {self._cycle}: requesting fresh lashing tools",
            )

        # Primary: secure container from pending queue (crane completions)
        if pending_containers:
            container = pending_containers[0]
            cid = container.get("container_id", "UNKNOWN")
            return AgentDecision(
                action="secure_container",
                arguments={
                    "container_id": cid,
                    "lashing_type": "twist_lock",
                },
                reasoning=f"DryRun cycle {self._cycle}: securing container {cid}",
            )

        # Simulate securing a container every other cycle
        if self._cycle % 2 == 0:
            cid = f"MSCU-{4472891 + (self._cycle % 20)}"
            return AgentDecision(
                action="secure_container",
                arguments={
                    "container_id": cid,
                    "lashing_type": "lashing_rod",
                },
                reasoning=f"DryRun cycle {self._cycle}: securing container {cid}",
            )

        return AgentDecision(
            action="wait",
            arguments={"reason": "Waiting for crane clear signal"},
            reasoning=f"DryRun cycle {self._cycle}: lashing crew idle, awaiting crane completion",
        )

    def _decide_signaler(self, observed_state: dict) -> AgentDecision:
        tasking = observed_state.get("tasking", {})
        crane_state = tasking.get("crane_state", "idle")
        ground_clear = tasking.get("ground_clear", True)

        # Every 8th cycle: report a hazard (simulate intermittent obstruction)
        if self._cycle % 8 == 0:
            return AgentDecision(
                action="report_hazard",
                arguments={
                    "hazard_type": "personnel",
                    "location": "under crane boom, bay 3",
                },
                reasoning=f"DryRun cycle {self._cycle}: spotted personnel in lift zone",
            )

        # If crane is actively lifting, monitor and signal
        if crane_state in ("LIFTING", "LOWERING"):
            return AgentDecision(
                action="signal_crane",
                arguments={
                    "signal_type": "CLEAR" if ground_clear else "STOP",
                    "crane_id": tasking.get("assigned_crane", "crane-1"),
                },
                reasoning=f"DryRun cycle {self._cycle}: {'clear' if ground_clear else 'STOP'} signal during {crane_state}",
            )

        # Default: confirm ground clear for next operation
        if self._cycle % 2 == 0:
            return AgentDecision(
                action="confirm_ground_clear",
                arguments={},
                reasoning=f"DryRun cycle {self._cycle}: confirming ground zone clear",
            )

        return AgentDecision(
            action="signal_crane",
            arguments={
                "signal_type": "CLEAR",
                "crane_id": tasking.get("assigned_crane", "crane-1"),
            },
            reasoning=f"DryRun cycle {self._cycle}: routine clear signal",
        )

    def _decide_gate_manager(self, observed_state: dict) -> AgentDecision:
        tasking = observed_state.get("tasking", {})
        gate_statuses = tasking.get("gate_statuses", [])
        truck_queue = tasking.get("truck_queue", [])
        yard_containers = tasking.get("yard_ready_containers", [])
        queue_length = len(truck_queue)

        # Every 4 cycles: produce a gate summary
        if self._cycle % 4 == 0:
            total_throughput = sum(
                g.get("trucks_per_hour", 0) for g in gate_statuses
            )
            return AgentDecision(
                action="update_gate_summary",
                arguments={
                    "trucks_per_hour": total_throughput,
                    "queue_length": queue_length,
                    "ready_containers": len(yard_containers),
                    "summary": (
                        f"{len(gate_statuses)} gates reporting, "
                        f"{total_throughput} trucks/hr, "
                        f"{queue_length} in queue"
                    ),
                },
                reasoning=(
                    f"DryRun cycle {self._cycle}: periodic gate summary — "
                    f"{total_throughput} trucks/hr, {queue_length} queued"
                ),
            )

        # Queue backup: prioritize fast lane
        if queue_length > 10:
            return AgentDecision(
                action="manage_truck_queue",
                arguments={
                    "action": "prioritize_fast_lane",
                    "reason": (
                        f"Queue backup detected ({queue_length} trucks), "
                        f"prioritizing pre-cleared trucks"
                    ),
                },
                reasoning=(
                    f"DryRun cycle {self._cycle}: queue exceeds threshold, "
                    f"reordering for efficiency"
                ),
            )

        # Release containers from yard to gate when trucks are waiting
        if yard_containers and queue_length > 0:
            container_id = (
                yard_containers[0]
                if isinstance(yard_containers[0], str)
                else yard_containers[0].get("container_id", "UNKNOWN")
            )
            return AgentDecision(
                action="release_container",
                arguments={
                    "container_id": container_id,
                    "gate_lane": "outbound-1",
                },
                reasoning=(
                    f"DryRun cycle {self._cycle}: container ready and truck "
                    f"waiting, releasing {container_id} from yard"
                ),
            )

        # Every 8 cycles: schedule rail load
        if self._cycle % 8 == 0:
            rail_containers = [
                (c if isinstance(c, str) else c.get("container_id", "UNKNOWN"))
                for c in yard_containers[:5]
            ]
            return AgentDecision(
                action="schedule_rail_load",
                arguments={
                    "rail_slot": "rail-1",
                    "containers": rail_containers,
                },
                reasoning=(
                    f"DryRun cycle {self._cycle}: periodic rail load "
                    f"coordination, {len(rail_containers)} containers"
                ),
            )

        return AgentDecision(
            action="wait",
            arguments={"reason": "Monitoring — no immediate gate operations needed"},
            reasoning=f"DryRun cycle {self._cycle}: gate manager idle, watching gate status",
        )

    def _decide_gate_scanner(self, observed_state: dict) -> AgentDecision:
        tasking = observed_state.get("tasking", {})
        truck = tasking.get("current_truck")

        if not truck:
            return AgentDecision(
                action="wait",
                arguments={"reason": "No truck in scan position"},
                reasoning=f"DryRun cycle {self._cycle}: gate scanner idle",
            )

        container_id = truck.get("container_id", f"MSCU-{4472891 + (self._cycle % 20)}")
        declared_weight = truck.get("declared_weight_tons", 25.0)
        # Simulate occasional weight anomaly
        measured_weight = declared_weight + (0.5 if self._cycle % 7 == 0 else 0.1)
        damage = self._cycle % 13 == 0

        return AgentDecision(
            action="scan_container",
            arguments={
                "container_id": container_id,
                "measured_weight_tons": round(measured_weight, 1),
                "declared_weight_tons": declared_weight,
                "weight_within_tolerance": abs(measured_weight - declared_weight) / declared_weight <= 0.05,
                "damage_detected": damage,
                "confidence": 0.98,
            },
            reasoning=(
                f"DryRun cycle {self._cycle}: scanning container {container_id}, "
                f"weight {measured_weight:.1f}t (declared {declared_weight}t)"
                + (", DAMAGE DETECTED" if damage else "")
            ),
        )

    def _decide_rfid_reader(self, observed_state: dict) -> AgentDecision:
        tasking = observed_state.get("tasking", {})
        truck = tasking.get("current_truck")

        if not truck:
            return AgentDecision(
                action="wait",
                arguments={"reason": "No truck in read position"},
                reasoning=f"DryRun cycle {self._cycle}: RFID reader idle",
            )

        container_id = truck.get("container_id", f"MSCU-{4472891 + (self._cycle % 20)}")

        return AgentDecision(
            action="scan_container",
            arguments={
                "container_id": container_id,
                "epc_tag": container_id,
                "match": True,
                "reading_type": "rfid",
            },
            reasoning=f"DryRun cycle {self._cycle}: RFID read {container_id}",
        )

    def _decide_gate_worker(self, observed_state: dict) -> AgentDecision:
        tasking = observed_state.get("tasking", {})
        truck = tasking.get("current_truck")
        trucks_processed = tasking.get("trucks_processed", 0)

        if not truck:
            return AgentDecision(
                action="wait",
                arguments={"reason": "No truck in processing position"},
                reasoning=f"DryRun cycle {self._cycle}: gate worker idle",
            )

        container_id = truck.get("container_id", f"MSCU-{4472891 + (self._cycle % 20)}")

        # Every 3rd cycle: verify documents first
        if self._cycle % 3 == 0:
            return AgentDecision(
                action="verify_documents",
                arguments={
                    "container_id": container_id,
                    "customs_cleared": True,
                    "bill_of_lading": True,
                    "documents_valid": True,
                },
                reasoning=f"DryRun cycle {self._cycle}: verifying docs for {container_id}",
            )

        # Every 5th cycle: inspect seal
        if self._cycle % 5 == 0:
            return AgentDecision(
                action="inspect_seal",
                arguments={
                    "container_id": container_id,
                    "seal_intact": True,
                    "seal_number_match": True,
                },
                reasoning=f"DryRun cycle {self._cycle}: inspecting seal for {container_id}",
            )

        # Default: process truck (release)
        return AgentDecision(
            action="process_truck",
            arguments={
                "container_id": container_id,
                "action": "release",
                "gate_lane": tasking.get("gate_lane", "gate-a-1"),
            },
            reasoning=(
                f"DryRun cycle {self._cycle}: releasing truck with {container_id} "
                f"(total processed: {trucks_processed + 1})"
            ),
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


class HybridProvider(LLMProvider):
    """SLM for routine decisions, escalates to API for complex ones.

    Checks the observed state for escalation keywords (hazmat, certification,
    safety, emergency). If found, routes to the API provider; otherwise uses
    the local SLM for low-latency decisions.
    """

    def __init__(
        self,
        slm: LLMProvider,
        api: LLMProvider,
        escalation_keywords: list[str] | None = None,
    ):
        self.slm = slm
        self.api = api
        self.escalation_keywords = [
            k.lower() for k in (escalation_keywords or [
                "hazmat", "certification", "safety", "emergency", "incident",
            ])
        ]
        self.slm_calls = 0
        self.api_calls = 0

    def _needs_escalation(self, observed_state: dict) -> bool:
        """Check if state contains complexity markers requiring API reasoning."""
        state_text = json.dumps(observed_state, default=str).lower()
        return any(kw in state_text for kw in self.escalation_keywords)

    async def decide(self, persona: str, observed_state: dict, available_tools: list[dict]) -> AgentDecision:
        if self._needs_escalation(observed_state):
            self.api_calls += 1
            logger.info("Hybrid: escalating to API (complex decision)")
            return await self.api.decide(persona, observed_state, available_tools)
        self.slm_calls += 1
        return await self.slm.decide(persona, observed_state, available_tools)


@dataclass
class TierConfig:
    """Parsed tier configuration from TOML."""
    tiers: dict[str, dict]      # tier_name → provider settings
    role_mapping: dict[str, str]  # role → tier_name


def load_tier_config(config_path: str) -> TierConfig:
    """Load tiered LLM config from a TOML file."""
    try:
        import tomllib
    except ModuleNotFoundError:
        import tomli as tomllib  # Python < 3.11 fallback

    from pathlib import Path
    text = Path(config_path).read_bytes()
    data = tomllib.loads(text.decode())

    tiers = {}
    for tier_name, tier_data in data.get("tiers", {}).items():
        tiers[tier_name] = dict(tier_data)

    role_mapping = dict(data.get("roles", {}))
    return TierConfig(tiers=tiers, role_mapping=role_mapping)


def create_provider_from_tier(tier_config: TierConfig, role: str) -> LLMProvider:
    """Create an LLM provider for a role based on tier configuration."""
    tier_name = tier_config.role_mapping.get(role)
    if tier_name is None:
        logger.warning("No tier mapping for role %r, falling back to dry-run", role)
        return DryRunProvider(role=role)

    tier = tier_config.tiers.get(tier_name)
    if tier is None:
        raise ValueError(f"Tier {tier_name!r} referenced by role {role!r} not defined in config")

    provider_type = tier.get("provider", "dry-run")

    if tier_name == "hybrid" or "escalation_provider" in tier:
        slm = create_provider(
            provider_type,
            role=role,
            model=tier.get("model"),
            base_url=tier.get("base_url"),
        )
        esc_provider = tier.get("escalation_provider", "anthropic")
        api = create_provider(
            esc_provider,
            role=role,
            model=tier.get("escalation_model"),
        )
        keywords = tier.get("escalation_keywords")
        return HybridProvider(slm=slm, api=api, escalation_keywords=keywords)

    return create_provider(
        provider_type,
        role=role,
        model=tier.get("model"),
        base_url=tier.get("base_url"),
    )


def create_provider(provider_type: str = "anthropic", **kwargs) -> LLMProvider:
    """Factory for LLM providers."""
    # DryRunProvider accepts 'role'; other providers don't — pop it for safety
    role = kwargs.pop("role", "crane")
    # Strip None values so providers use their defaults
    kwargs = {k: v for k, v in kwargs.items() if v is not None}
    if provider_type == "anthropic":
        return AnthropicProvider(**kwargs)
    elif provider_type in ("ollama", "openai"):
        return OllamaProvider(**kwargs)
    elif provider_type == "dry-run":
        return DryRunProvider(role=role, **kwargs)
    else:
        raise ValueError(f"Unknown provider: {provider_type}. Use 'anthropic', 'ollama', or 'dry-run'.")
