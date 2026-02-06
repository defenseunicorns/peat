"""
Agent OODA Loop Runner

Implements the continuous Observe-Orient-Decide-Act loop for port agents.
Each cycle:
  1. OBSERVE  — Read HIVE state via MCP resources
  2. ORIENT   — LLM reasons about situation (persona + context)
  3. DECIDE   — LLM selects action from available MCP tools
  4. ACT      — Call MCP tool → updates HIVE state
  5. WAIT     — Simulated operation time passes
  6. REPEAT

Simulation clock: configurable time compression (default 60x: 1 real second = 1 sim minute)
"""

from __future__ import annotations

import asyncio
import json
import logging
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

from .llm import LLMProvider, AgentDecision

logger = logging.getLogger(__name__)


@dataclass
class SimulationClock:
    """
    Manages simulation time compression.
    Default: 60x (1 real second = 1 simulated minute)
    """
    compression_ratio: float = 60.0
    start_real_time: float = field(default_factory=time.time)
    start_sim_time: float = 0.0  # Simulated minutes from T=0

    @property
    def sim_minutes(self) -> float:
        """Current simulated time in minutes from T=0."""
        real_elapsed = time.time() - self.start_real_time
        return self.start_sim_time + (real_elapsed * self.compression_ratio / 60.0)

    @property
    def sim_time_str(self) -> str:
        """Human-readable sim time."""
        total_min = self.sim_minutes
        hours = int(total_min // 60)
        minutes = int(total_min % 60)
        return f"T+{hours:02d}:{minutes:02d}"

    async def wait_sim_minutes(self, sim_minutes: float):
        """Wait for a number of simulated minutes."""
        real_seconds = (sim_minutes * 60.0) / self.compression_ratio
        await asyncio.sleep(real_seconds)


@dataclass
class CycleMetrics:
    """Metrics for a single OODA cycle."""
    cycle_number: int
    sim_time: str
    observe_duration_ms: float
    decide_duration_ms: float
    act_duration_ms: float
    action: str
    arguments: dict
    reasoning: str
    success: bool
    result: str = ""


class AgentLoop:
    """
    The OODA loop runner for a port operations agent.

    Connects to the MCP bridge server to read HIVE state and execute actions.
    Uses an LLM provider for the Orient/Decide phases.
    """

    def __init__(
        self,
        node_id: str,
        persona_path: str,
        llm: LLMProvider,
        mcp_client: Any,  # MCP ClientSession
        clock: SimulationClock | None = None,
        max_cycles: int = 100,
        cycle_delay_sim_minutes: float = 1.5,  # ~1.5 sim minutes per OODA loop
        dashboard: Any = None,  # Optional LiveDashboard
    ):
        self.node_id = node_id
        self.persona = Path(persona_path).read_text()
        self.llm = llm
        self.mcp = mcp_client
        self.clock = clock or SimulationClock()
        self.max_cycles = max_cycles
        self.cycle_delay_sim_minutes = cycle_delay_sim_minutes
        self.cycle_count = 0
        self.metrics: list[CycleMetrics] = []
        self.dashboard = dashboard

    async def observe(self) -> dict[str, Any]:
        """OBSERVE — Read all HIVE state via MCP resources."""
        state = {}

        resources = {
            "my_capabilities": "hive://my-capabilities",
            "team_state": "hive://team-state",
            "container_queue": "hive://container-queue",
            "tasking": "hive://tasking",
        }

        for key, uri in resources.items():
            try:
                result = await self.mcp.read_resource(uri)
                # MCP returns content as a list; extract text
                if hasattr(result, "contents") and result.contents:
                    text = result.contents[0].text
                else:
                    text = str(result)
                state[key] = json.loads(text) if isinstance(text, str) else text
            except Exception as e:
                logger.warning(f"Failed to read {uri}: {e}")
                state[key] = {"error": str(e)}

        return state

    async def decide(self, observed_state: dict) -> AgentDecision:
        """ORIENT + DECIDE — LLM reasons and selects action."""
        # Get available tools from MCP
        tools_result = await self.mcp.list_tools()
        tools = []
        for tool in tools_result.tools:
            tools.append({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.inputSchema,
            })

        # Add sim time context to the state
        observed_state["_simulation_time"] = self.clock.sim_time_str
        observed_state["_cycle_number"] = self.cycle_count

        return await self.llm.decide(self.persona, observed_state, tools)

    async def act(self, decision: AgentDecision) -> str:
        """ACT — Execute the decided action via MCP tool call."""
        if decision.action == "wait":
            reason = decision.arguments.get("reason", "No action needed")
            logger.info(f"[{self.clock.sim_time_str}] {self.node_id} WAIT: {reason}")
            return f"Waited: {reason}"

        try:
            result = await self.mcp.call_tool(decision.action, decision.arguments)
            # Extract text from result
            if hasattr(result, "content") and result.content:
                text = result.content[0].text
            else:
                text = str(result)
            logger.info(
                f"[{self.clock.sim_time_str}] {self.node_id} ACT: "
                f"{decision.action}({json.dumps(decision.arguments)}) -> {text}"
            )
            return text
        except Exception as e:
            logger.error(f"[{self.clock.sim_time_str}] {self.node_id} ACT FAILED: {e}")
            return f"Error: {e}"

    async def run_cycle(self) -> CycleMetrics:
        """Execute one complete OODA cycle."""
        self.cycle_count += 1
        logger.info(
            f"\n{'='*60}\n"
            f"[{self.clock.sim_time_str}] {self.node_id} — CYCLE {self.cycle_count}\n"
            f"{'='*60}"
        )

        # OBSERVE
        t0 = time.time()
        state = await self.observe()
        observe_ms = (time.time() - t0) * 1000

        logger.info(f"[{self.clock.sim_time_str}] OBSERVE complete ({observe_ms:.0f}ms)")

        if self.dashboard:
            self.dashboard.update_observe(state, self.clock.sim_time_str, self.cycle_count)

        # ORIENT + DECIDE
        t1 = time.time()
        decision = await self.decide(state)
        decide_ms = (time.time() - t1) * 1000

        logger.info(
            f"[{self.clock.sim_time_str}] DECIDE: {decision.action} "
            f"| Reasoning: {decision.reasoning[:100]}... ({decide_ms:.0f}ms)"
        )

        if self.dashboard:
            self.dashboard.update_decide(decision.action, decision.arguments, decision.reasoning, decide_ms)

        # ACT
        t2 = time.time()
        result = await self.act(decision)
        act_ms = (time.time() - t2) * 1000

        success = "Error" not in result

        metrics = CycleMetrics(
            cycle_number=self.cycle_count,
            sim_time=self.clock.sim_time_str,
            observe_duration_ms=observe_ms,
            decide_duration_ms=decide_ms,
            act_duration_ms=act_ms,
            action=decision.action,
            arguments=decision.arguments,
            reasoning=decision.reasoning,
            success=success,
            result=result,
        )
        self.metrics.append(metrics)

        if self.dashboard:
            self.dashboard.update_act(result, observe_ms, decide_ms, act_ms, success)

        # Log METRICS in structured format matching hive-sim pattern
        print(json.dumps({
            "type": "METRICS",
            "event": "ooda_cycle",
            "node_id": self.node_id,
            "cycle": self.cycle_count,
            "sim_time": self.clock.sim_time_str,
            "action": decision.action,
            "observe_ms": round(observe_ms, 1),
            "decide_ms": round(decide_ms, 1),
            "act_ms": round(act_ms, 1),
            "total_ms": round(observe_ms + decide_ms + act_ms, 1),
            "success": metrics.success,
            "timestamp_us": int(time.time() * 1_000_000),
        }), flush=True)

        return metrics

    async def run(self) -> list[CycleMetrics]:
        """Run the full OODA loop until max_cycles or completion."""
        logger.info(
            f"Starting agent loop: {self.node_id}\n"
            f"  Persona: {self.persona[:80]}...\n"
            f"  Max cycles: {self.max_cycles}\n"
            f"  Cycle delay: {self.cycle_delay_sim_minutes} sim minutes\n"
            f"  Clock compression: {self.clock.compression_ratio}x"
        )

        for _ in range(self.max_cycles):
            try:
                cycle = await self.run_cycle()

                # Check termination conditions
                if not cycle.success and "Error" in cycle.result:
                    logger.warning(f"Cycle failed: {cycle.result}")
                    # Continue — agents should be resilient

                # WAIT — simulated operation time
                await self.clock.wait_sim_minutes(self.cycle_delay_sim_minutes)

            except KeyboardInterrupt:
                logger.info("Agent loop interrupted by user")
                break
            except Exception as e:
                logger.error(f"Unexpected error in cycle {self.cycle_count}: {e}")
                await asyncio.sleep(1)

        # Fetch full HIVE state dump for dashboard/summary
        if self.dashboard:
            try:
                dump_result = await self.mcp.read_resource("hive://debug/state-dump")
                if hasattr(dump_result, "contents") and dump_result.contents:
                    dump_text = dump_result.contents[0].text
                else:
                    dump_text = str(dump_result)
                dump = json.loads(dump_text) if isinstance(dump_text, str) else dump_text
                self.dashboard.update_hive_dump(
                    dump.get("documents", {}),
                    dump.get("events", []),
                )
            except Exception as e:
                logger.warning(f"Failed to fetch state dump: {e}")

            self.dashboard.cleanup()

        # Summary
        total_actions = sum(1 for m in self.metrics if m.action != "wait")
        total_waits = sum(1 for m in self.metrics if m.action == "wait")
        avg_decide = (
            sum(m.decide_duration_ms for m in self.metrics) / len(self.metrics)
            if self.metrics else 0
        )

        if not self.dashboard:
            logger.info(
                f"\n{'='*60}\n"
                f"Agent loop complete: {self.node_id}\n"
                f"  Total cycles: {self.cycle_count}\n"
                f"  Actions: {total_actions}\n"
                f"  Waits: {total_waits}\n"
                f"  Avg decide time: {avg_decide:.0f}ms\n"
                f"  Sim time: {self.clock.sim_time_str}\n"
                f"{'='*60}"
            )

        return self.metrics
