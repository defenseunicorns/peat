"""
Port Agent Main — Phase 0 & Phase 1a entrypoint

Phase 0 (single mode): Runs a single crane agent with MCP bridge subprocess.
Phase 1a (multi mode):  Runs multiple agents with shared in-process HIVE state.

Usage:
  # Phase 0 — single agent
  python -m port_agent.main --node-id crane-1 --provider anthropic
  python -m port_agent.main --node-id crane-1 --provider dry-run

  # Phase 1a — multi-agent
  python -m port_agent.main --mode multi --agents 2c1a --provider dry-run
"""

import argparse
import asyncio
import json
import logging
import os
import sys
from pathlib import Path

from mcp import ClientSession
from mcp.client.stdio import stdio_client, StdioServerParameters

from .dashboard import LiveDashboard, render_post_run_summary
from .llm import create_provider, load_tier_config
from .loop import AgentLoop, SimulationClock


logger = logging.getLogger(__name__)


async def run_phase0_inprocess(args):
    """
    Phase 0: Run agent with in-process MCP bridge.

    Instead of launching a separate MCP server process, we run the bridge
    server in-process using MCP's stdio transport with subprocess.
    """
    # Determine paths
    script_dir = Path(__file__).parent.parent.parent.parent  # port-ops/
    persona_path = script_dir / "personas" / f"{args.persona}.md"
    bridge_module = "port_agent_bridge.server"

    if not persona_path.exists():
        logger.error(f"Persona file not found: {persona_path}")
        sys.exit(1)

    # Entity configuration
    entity_config = {
        "lift_capacity_tons": args.lift_capacity,
        "reach_rows": args.reach_rows,
        "moves_per_hour": args.moves_per_hour,
        "hold": args.hold,
        "berth": args.berth,
        "vessel": args.vessel,
        "hazmat_classes": [1, 3, 8, 9],
        "hazmat_cert_valid": True,
        "queue_size": args.queue_size,
        "hazmat_count": args.hazmat_count,
    }

    # Create LLM provider
    provider_kwargs = {}
    if args.model:
        provider_kwargs["model"] = args.model
    if args.provider == "ollama":
        provider_kwargs["base_url"] = args.ollama_url
    llm = create_provider(args.provider, **provider_kwargs)

    # Create simulation clock
    clock = SimulationClock(compression_ratio=args.time_compression)

    logger.info(f"Phase 0 Agent Runner")
    logger.info(f"  Node ID: {args.node_id}")
    logger.info(f"  Persona: {persona_path.name}")
    logger.info(f"  LLM Provider: {args.provider} ({args.model or 'default'})")
    logger.info(f"  Time compression: {args.time_compression}x")
    logger.info(f"  Max cycles: {args.max_cycles}")
    logger.info(f"  Entity config: {json.dumps(entity_config, indent=2)}")

    # Launch MCP bridge as a subprocess
    bridge_src = script_dir / "bridge" / "src"

    config_json = json.dumps(entity_config)

    server_params = StdioServerParameters(
        command=sys.executable,
        args=[
            "-c",
            (
                "import asyncio, sys, json, os; "
                f"sys.path.insert(0, '{bridge_src}'); "
                "from port_agent_bridge.server import run_server; "
                "config = json.loads(os.environ['ENTITY_CONFIG']); "
                f"asyncio.run(run_server('{args.node_id}', config))"
            ),
        ],
        env={**os.environ, "ENTITY_CONFIG": config_json},
    )

    async with stdio_client(server_params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()

            logger.info("MCP bridge connected and initialized")

            # Create dashboard if requested
            dashboard = None
            if args.dashboard:
                dashboard = LiveDashboard(
                    node_id=args.node_id,
                    max_cycles=args.max_cycles,
                    enabled=True,
                )

            # Create and run the agent loop
            agent = AgentLoop(
                node_id=args.node_id,
                persona_path=str(persona_path),
                llm=llm,
                mcp_client=session,
                clock=clock,
                max_cycles=args.max_cycles,
                cycle_delay_sim_minutes=args.cycle_delay,
                dashboard=dashboard,
            )

            metrics = await agent.run()

            # Render post-run summary
            if dashboard:
                summary = render_post_run_summary(dashboard.state)
                sys.stderr.write(summary)
                sys.stderr.flush()

            # Write metrics summary
            if args.metrics_file:
                metrics_data = [
                    {
                        "cycle": m.cycle_number,
                        "sim_time": m.sim_time,
                        "action": m.action,
                        "arguments": m.arguments,
                        "reasoning": m.reasoning[:200],
                        "observe_ms": m.observe_duration_ms,
                        "decide_ms": m.decide_duration_ms,
                        "act_ms": m.act_duration_ms,
                        "success": m.success,
                        "result": m.result[:200],
                    }
                    for m in metrics
                ]
                Path(args.metrics_file).write_text(json.dumps(metrics_data, indent=2))
                logger.info(f"Metrics written to {args.metrics_file}")


async def run_phase1a_multi(args):
    """
    Run multiple agents with shared in-process HIVE state.
    Supports both single-hold (Phase 1c) and multi-hold (Phase 2) compositions.
    """
    from .orchestrator import Orchestrator, OrchestratorConfig, parse_agent_composition

    script_dir = Path(__file__).parent.parent.parent.parent  # port-ops/
    personas_dir = script_dir / "personas"

    # Load tiered LLM config if specified
    tier_config = None
    if args.llm_config:
        tier_config = load_tier_config(args.llm_config)
        logger.info(f"Loaded tiered LLM config from {args.llm_config}")
        logger.info(f"  Tiers: {', '.join(tier_config.tiers.keys())}")
        logger.info(f"  Role mappings: {tier_config.role_mapping}")

    agent_specs, num_holds, hold_nums = parse_agent_composition(
        args.agents, provider=args.provider, model=args.model
    )

    config = OrchestratorConfig(
        hold_id=f"hold-{hold_nums[0]}",
        hold_num=hold_nums[0],
        berth=args.berth,
        vessel=args.vessel,
        queue_size=args.queue_size,
        hazmat_count=args.hazmat_count,
        max_cycles=args.max_cycles,
        cycle_delay_sim_minutes=args.cycle_delay,
        time_compression=args.time_compression,
        agents=agent_specs,
        num_holds=num_holds,
        hold_nums=hold_nums,
        tier_config=tier_config,
    )

    orch = Orchestrator(config)
    orch.initialize_state()
    orch.create_agents(personas_dir)
    await orch.run()


def main():
    parser = argparse.ArgumentParser(description="Port Agent — HIVE Port Operations Simulation")

    # Mode selection
    parser.add_argument("--mode", default="single", choices=["single", "multi"],
                        help="Run mode: single (Phase 0) or multi (Phase 1a)")
    parser.add_argument("--agents", default="2c1a",
                        help="Agent composition for multi mode (e.g., 2c1a = 2 cranes + 1 aggregator)")

    # Common args
    parser.add_argument("--node-id", default="crane-1", help="Node identifier (e.g., crane-1)")
    parser.add_argument("--persona", default="gantry-crane", help="Persona file name (without .md)")
    parser.add_argument("--provider", default="anthropic", choices=["anthropic", "ollama", "dry-run"],
                        help="LLM provider (dry-run for offline testing)")
    parser.add_argument("--model", default=None, help="Model name (provider-specific)")
    parser.add_argument("--ollama-url", default="http://localhost:11434/v1",
                        help="Ollama API base URL")
    parser.add_argument("--max-cycles", type=int, default=20, help="Maximum OODA cycles")
    parser.add_argument("--cycle-delay", type=float, default=1.5,
                        help="Simulated minutes between cycles")
    parser.add_argument("--time-compression", type=float, default=60.0,
                        help="Time compression ratio (60 = 1 real sec = 1 sim min)")
    parser.add_argument("--lift-capacity", type=float, default=65.0, help="Crane lift capacity (tons)")
    parser.add_argument("--reach-rows", type=int, default=22, help="Crane outreach (rows)")
    parser.add_argument("--moves-per-hour", type=int, default=30, help="Rated moves/hour")
    parser.add_argument("--hold", type=int, default=3, help="Assigned hold number")
    parser.add_argument("--berth", default="berth-5", help="Assigned berth")
    parser.add_argument("--vessel", default="MV Ever Forward", help="Vessel name")
    parser.add_argument("--queue-size", type=int, default=20, help="Container queue size for Phase 0")
    parser.add_argument("--hazmat-count", type=int, default=3, help="Number of hazmat containers")
    parser.add_argument("--metrics-file", default=None, help="Path to write metrics JSON")
    parser.add_argument("--dashboard", action="store_true",
                        help="Enable live terminal dashboard (clears screen each cycle)")
    parser.add_argument("--llm-config", default=None,
                        help="Path to tiered LLM config TOML (overrides --provider per role)")
    parser.add_argument("--log-level", default="INFO", help="Logging level")

    args = parser.parse_args()

    # When dashboard is active, suppress logging to avoid fighting with screen redraws
    log_level = logging.WARNING if args.dashboard else getattr(logging, args.log_level.upper())
    logging.basicConfig(
        level=log_level,
        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
        stream=sys.stderr,
    )

    if args.mode == "multi":
        asyncio.run(run_phase1a_multi(args))
    else:
        asyncio.run(run_phase0_inprocess(args))


if __name__ == "__main__":
    main()
