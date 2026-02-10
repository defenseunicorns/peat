#!/usr/bin/env python3
"""
Lab Orchestrator - Simple HTTP service for coordinating containerlab simulations.

Nodes POST their status to this service. The orchestrator tracks:
- Node registration and readiness
- Sync/convergence status
- Errors and failures
- Test lifecycle (waiting -> ready -> running -> complete)
- Equipment agent LLM decisions (via tiered provider system)

Scenario mode (--scenario):
- Loads ADR-051 scenario timeline from JSON
- Injects events at specified cycle counts
- Drives bridge_api entity state mutations
- Produces dry-run decisions via llm.py

Usage:
    python3 orchestrator/main.py --expected-nodes 447 --port 8080
    python3 orchestrator/main.py --expected-nodes 12 --provider ollama --ollama-model llama3:8b
    python3 orchestrator/main.py --scenario scenarios/mv_ever_forward.json --port 8080

Endpoints:
    POST /register          - Node registers itself (called on startup)
    POST /ready             - Node reports it's ready (sync established)
    POST /metrics           - Node pushes metrics
    POST /error             - Node reports an error
    POST /decide            - Equipment agent requests LLM decision
    GET  /status            - Dashboard JSON
    GET  /                  - Human-readable dashboard
    POST /reset             - Reset all state for new test run
    GET  /scenario/status   - Scenario timeline and progress
    GET  /scenario/state    - Full entity state snapshot
    GET  /scenario/metrics  - Berth-level metrics
    POST /scenario/advance  - Manually advance scenario cycle
"""

import argparse
import json
import logging
import os
import sys
import threading
import time
from collections import defaultdict
from dataclasses import dataclass, field, asdict
from datetime import datetime
from http.server import HTTPServer, BaseHTTPRequestHandler
from pathlib import Path
from typing import Optional
from urllib.parse import parse_qs, urlparse

from llm import (
    EQUIPMENT_LLM_TIERS,
    LlmProvider,
    LlmResponse,
    create_provider,
    get_system_prompt,
)

logger = logging.getLogger(__name__)


# Realistic worker name pool (ILA Local 1414 style, per ADR-051 entity model)
# Pattern: worker_{surname}_{initial} as node ID, "Surname, I" as display name
WORKER_NAME_POOL = [
    ("martinez_j", "Martinez, J"),
    ("chen_l", "Chen, L"),
    ("thompson_r", "Thompson, R"),
    ("williams_d", "Williams, D"),
    ("garcia_m", "Garcia, M"),
    ("johnson_k", "Johnson, K"),
    ("brown_a", "Brown, A"),
    ("davis_s", "Davis, S"),
    ("wilson_p", "Wilson, P"),
    ("anderson_t", "Anderson, T"),
]


def assign_worker_name(index: int) -> tuple[str, str]:
    """Assign a realistic worker name from the pool.

    Returns (worker_id, display_name) tuple. Cycles through the pool
    for indices beyond pool size.
    """
    entry = WORKER_NAME_POOL[index % len(WORKER_NAME_POOL)]
    suffix = f"_{index // len(WORKER_NAME_POOL) + 1}" if index >= len(WORKER_NAME_POOL) else ""
    worker_id = f"worker_{entry[0]}{suffix}"
    display_name = entry[1]
    return worker_id, display_name


@dataclass
class NodeState:
    node_id: str
    registered_at: str
    ready_at: Optional[str] = None
    last_seen: Optional[str] = None
    backend: str = "unknown"
    role: str = "unknown"
    display_name: Optional[str] = None
    worker_id: Optional[str] = None
    llm_tier: str = "none"  # none, local_slm, api
    equipment_type: str = ""  # crane, tractor, etc.
    errors: list = field(default_factory=list)
    metrics: dict = field(default_factory=dict)
    llm_decisions: list = field(default_factory=list)


class ScenarioEngine:
    """Loads scenario timeline and fires events at specified cycle counts."""

    def __init__(self, scenario_path: str):
        self.scenario_path = Path(scenario_path)
        self.scenario_dir = self.scenario_path.parent
        self.scenario: dict = {}
        self.phases: list[dict] = []
        self.all_events: list[dict] = []  # Flattened, sorted by absolute cycle
        self.fired_events: list[dict] = []
        self.decisions: list[dict] = []
        self.cycle: int = 0
        self.running: bool = False
        self.lock = threading.Lock()
        self._cycle_thread: Optional[threading.Thread] = None
        self._entity_manager = None
        self._decision_engine = None

    def load(self) -> None:
        """Load scenario definition and all event files."""
        with open(self.scenario_path) as f:
            self.scenario = json.load(f)

        self.phases = self.scenario.get("phases", [])
        cycle_duration_ms = self.scenario.get("cycle_duration_ms", 5000)

        # Load and flatten all events with absolute cycle numbers
        for phase in self.phases:
            events_file = phase.get("events_file")
            if not events_file:
                continue
            events_path = self.scenario_dir / events_file
            if not events_path.exists():
                print(f"Warning: events file not found: {events_path}")
                continue
            with open(events_path) as f:
                phase_data = json.load(f)
            trigger_cycle = phase.get("trigger_cycle", 0)
            for event in phase_data.get("events", []):
                abs_cycle = trigger_cycle + event.get("cycle_offset", 0)
                self.all_events.append({
                    "absolute_cycle": abs_cycle,
                    "phase_id": phase["phase_id"],
                    "phase_name": phase["name"],
                    **event,
                })

        self.all_events.sort(key=lambda e: (e["absolute_cycle"], e.get("event_id", "")))
        print(f"Scenario loaded: {self.scenario.get('name', 'unknown')}")
        print(f"  {len(self.phases)} phases, {len(self.all_events)} events")
        print(f"  Cycle duration: {cycle_duration_ms}ms")

    def init_modules(self) -> None:
        """Initialize bridge_api and llm modules, then fire cycle-0 events."""
        # Import from same directory
        script_dir = os.path.dirname(os.path.abspath(__file__))
        if script_dir not in sys.path:
            sys.path.insert(0, script_dir)

        from bridge_api import EntityStateManager, dispatch_event
        from llm import DryRunDecisionEngine

        self._entity_manager = EntityStateManager()
        self._entity_manager.load_scenario_entities(self.scenario)
        self._decision_engine = DryRunDecisionEngine(self._entity_manager)
        self._dispatch_event = dispatch_event

        # Fire any events scheduled for cycle 0 (T=0 initialization)
        for event in self.all_events:
            if event["absolute_cycle"] == 0 and event not in self.fired_events:
                self._fire_event(event)

    def start(self, cycle_duration_ms: int = 5000) -> None:
        """Start automatic cycle advancement."""
        if self.running:
            return
        self.running = True
        interval = cycle_duration_ms / 1000.0
        self._cycle_thread = threading.Thread(
            target=self._cycle_loop, args=(interval,), daemon=True
        )
        self._cycle_thread.start()
        print(f"Scenario engine started (cycle interval: {interval}s)")

    def stop(self) -> None:
        self.running = False

    def _cycle_loop(self, interval: float) -> None:
        while self.running:
            time.sleep(interval)
            self.advance_cycle()

    def advance_cycle(self) -> list[dict]:
        """Advance one cycle. Fire any events scheduled for this cycle."""
        with self.lock:
            self.cycle += 1
            if self._entity_manager:
                self._entity_manager.cycle = self.cycle

            fired = []
            for event in self.all_events:
                if event["absolute_cycle"] == self.cycle and event not in self.fired_events:
                    result = self._fire_event(event)
                    fired.append({"event": event, "result": result})

            return fired

    def _fire_event(self, event: dict) -> dict:
        """Fire a single event: dispatch to bridge_api, then get LLM decisions."""
        self.fired_events.append(event)
        phase = event.get("phase_name", "unknown")
        eid = event.get("event_id", "?")
        etype = event.get("type", "?")
        print(f"  [cycle {self.cycle}] {phase} | {eid}: {etype} - {event.get('description', '')}")

        result = {}
        if self._entity_manager and self._dispatch_event:
            result = self._dispatch_event(self._entity_manager, event)

        # Get dry-run decisions for decision-worthy events
        if self._decision_engine:
            decisions = self._decision_engine.decide_on_event(event)
            for dec in decisions:
                dec_dict = dec.to_dict()
                dec_dict["triggered_by"] = eid
                dec_dict["cycle"] = self.cycle
                self.decisions.append(dec_dict)
                print(f"    -> Decision {dec.decision_id}: {dec.action} (confidence: {dec.confidence})")

        return result

    def get_status(self) -> dict:
        with self.lock:
            total_events = len(self.all_events)
            fired_count = len(self.fired_events)
            pending = [e for e in self.all_events if e not in self.fired_events]
            next_event = pending[0] if pending else None

            phases_status = []
            for phase in self.phases:
                phase_events = [e for e in self.all_events if e["phase_id"] == phase["phase_id"]]
                phase_fired = [e for e in self.fired_events if e["phase_id"] == phase["phase_id"]]
                phases_status.append({
                    "phase_id": phase["phase_id"],
                    "name": phase["name"],
                    "trigger_cycle": phase["trigger_cycle"],
                    "total_events": len(phase_events),
                    "fired_events": len(phase_fired),
                    "complete": len(phase_fired) == len(phase_events),
                })

            return {
                "scenario": self.scenario.get("name", "unknown"),
                "cycle": self.cycle,
                "running": self.running,
                "total_events": total_events,
                "fired_events": fired_count,
                "pending_events": total_events - fired_count,
                "progress_pct": round(100 * fired_count / total_events, 1) if total_events else 0,
                "next_event_cycle": next_event["absolute_cycle"] if next_event else None,
                "next_event_type": next_event.get("type") if next_event else None,
                "phases": phases_status,
                "decisions_made": len(self.decisions),
            }

    def get_entity_state(self) -> dict:
        if self._entity_manager:
            return self._entity_manager.get_all_state()
        return {}

    def get_berth_metrics(self) -> dict:
        if self._entity_manager:
            return self._entity_manager.get_berth_metrics()
        return {}

    def get_decisions(self) -> list[dict]:
        with self.lock:
            return list(self.decisions)

    def get_event_log(self) -> list[dict]:
        if self._entity_manager:
            return list(self._entity_manager.event_log)
        return []


class Orchestrator:
    def __init__(self, expected_nodes: int, llm_provider: Optional[LlmProvider] = None):
        self.expected_nodes = expected_nodes
        self.nodes: dict[str, NodeState] = {}
        self.test_start_time: Optional[str] = None
        self.test_state = "waiting"  # waiting -> deploying -> ready -> running -> complete
        self.lock = threading.Lock()
        self.llm_provider = llm_provider
        self.llm_decision_count = 0
        self.llm_total_latency_ms = 0.0
        self.scenario_engine: Optional[ScenarioEngine] = None

    def register(self, node_id: str, backend: str = "unknown", role: str = "unknown",
                 equipment_type: str = "") -> dict:
        with self.lock:
            now = datetime.utcnow().isoformat()
            # Determine LLM tier based on equipment type
            llm_tier = EQUIPMENT_LLM_TIERS.get(equipment_type, "none")
            if node_id not in self.nodes:
                # Assign realistic worker identity from the name pool
                worker_index = len(self.nodes)
                worker_id, display_name = assign_worker_name(worker_index)
                self.nodes[node_id] = NodeState(
                    node_id=node_id,
                    registered_at=now,
                    backend=backend,
                    role=role,
                    display_name=display_name,
                    worker_id=worker_id,
                    llm_tier=llm_tier,
                    equipment_type=equipment_type,
                )
            self.nodes[node_id].last_seen = now

            if self.test_state == "waiting":
                self.test_state = "deploying"
                self.test_start_time = now

            node = self.nodes[node_id]
            return {
                "status": "registered",
                "node_count": len(self.nodes),
                "worker_id": node.worker_id,
                "display_name": node.display_name,
                "llm_tier": llm_tier,
            }

    def mark_ready(self, node_id: str) -> dict:
        with self.lock:
            now = datetime.utcnow().isoformat()
            if node_id in self.nodes:
                self.nodes[node_id].ready_at = now
                self.nodes[node_id].last_seen = now

            ready_count = sum(1 for n in self.nodes.values() if n.ready_at)

            if ready_count >= self.expected_nodes and self.test_state == "deploying":
                self.test_state = "ready"

            return {"status": "ready", "ready_count": ready_count, "expected": self.expected_nodes}

    def report_metrics(self, node_id: str, metrics: dict) -> dict:
        with self.lock:
            now = datetime.utcnow().isoformat()
            if node_id in self.nodes:
                self.nodes[node_id].metrics.update(metrics)
                self.nodes[node_id].last_seen = now
            return {"status": "ok"}

    def report_error(self, node_id: str, error: str) -> dict:
        with self.lock:
            now = datetime.utcnow().isoformat()
            if node_id in self.nodes:
                self.nodes[node_id].errors.append({"time": now, "error": error})
                self.nodes[node_id].last_seen = now
            return {"status": "recorded"}

    def request_decision(self, node_id: str, prompt: str, context: dict = None) -> dict:
        """Equipment agent requests an LLM decision."""
        with self.lock:
            node = self.nodes.get(node_id)
            if not node:
                return {"status": "error", "error": "node not registered"}
            if node.llm_tier == "none":
                return {"status": "error", "error": "node has no LLM tier assigned"}
            if not self.llm_provider:
                return {"status": "error", "error": "no LLM provider configured"}

        # Run inference outside the lock
        equipment_type = node.equipment_type or "default"
        system_prompt = get_system_prompt(equipment_type)

        # Include context in prompt if provided
        full_prompt = prompt
        if context:
            ctx_str = json.dumps(context, indent=2)
            full_prompt = f"{prompt}\n\nOperational context:\n{ctx_str}"

        response = self.llm_provider.generate(full_prompt, system=system_prompt)

        with self.lock:
            now = datetime.utcnow().isoformat()
            decision = {
                "time": now,
                "prompt": prompt[:200],  # Truncate for storage
                "response": response.text[:500],
                "model": response.model,
                "latency_ms": response.latency_ms,
            }
            if node_id in self.nodes:
                self.nodes[node_id].llm_decisions.append(decision)
                self.nodes[node_id].last_seen = now
            self.llm_decision_count += 1
            self.llm_total_latency_ms += response.latency_ms

        return {
            "status": "ok",
            "decision": response.text,
            "model": response.model,
            "provider": response.provider,
            "latency_ms": round(response.latency_ms, 1),
        }

    def get_status(self) -> dict:
        with self.lock:
            registered = len(self.nodes)
            ready = sum(1 for n in self.nodes.values() if n.ready_at)
            errors = sum(len(n.errors) for n in self.nodes.values())

            # Group by role
            by_role = defaultdict(lambda: {"registered": 0, "ready": 0})
            for n in self.nodes.values():
                by_role[n.role]["registered"] += 1
                if n.ready_at:
                    by_role[n.role]["ready"] += 1

            # Group by backend
            by_backend = defaultdict(int)
            for n in self.nodes.values():
                by_backend[n.backend] += 1

            # Build worker roster with display names
            workers = {
                n.node_id: {
                    "worker_id": n.worker_id,
                    "display_name": n.display_name,
                    "role": n.role,
                    "ready": n.ready_at is not None,
                }
                for n in self.nodes.values()
            }

            # Group by LLM tier
            by_llm_tier = defaultdict(int)
            for n in self.nodes.values():
                by_llm_tier[n.llm_tier] += 1

            avg_llm_latency = (
                round(self.llm_total_latency_ms / self.llm_decision_count, 1)
                if self.llm_decision_count else 0.0
            )

            status = {
                "test_state": self.test_state,
                "test_start_time": self.test_start_time,
                "expected_nodes": self.expected_nodes,
                "registered": registered,
                "ready": ready,
                "progress_pct": round(100 * registered / self.expected_nodes, 1) if self.expected_nodes else 0,
                "ready_pct": round(100 * ready / self.expected_nodes, 1) if self.expected_nodes else 0,
                "total_errors": errors,
                "by_role": dict(by_role),
                "by_backend": dict(by_backend),
                "by_llm_tier": dict(by_llm_tier),
                "llm_provider": self.llm_provider.provider_name() if self.llm_provider else "none",
                "llm_decisions": self.llm_decision_count,
                "llm_avg_latency_ms": avg_llm_latency,
                "nodes_with_errors": [n.node_id for n in self.nodes.values() if n.errors],
                "workers": workers,
            }

            if self.scenario_engine:
                status["scenario"] = self.scenario_engine.get_status()

            return status

    def reset(self):
        with self.lock:
            self.nodes.clear()
            self.test_state = "waiting"
            self.test_start_time = None
            self.llm_decision_count = 0
            self.llm_total_latency_ms = 0.0
            if self.scenario_engine:
                self.scenario_engine.stop()
                self.scenario_engine = None


# Global orchestrator instance
orchestrator: Optional[Orchestrator] = None


class OrchestratorHandler(BaseHTTPRequestHandler):
    # Increase timeout for slow clients
    timeout = 5

    def log_message(self, format, *args):
        # Quieter logging - only log errors
        pass

    def log_error(self, format, *args):
        # Suppress broken pipe errors
        if "Broken pipe" not in str(args):
            super().log_error(format, *args)

    def send_json(self, data, status: int = 200):
        try:
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(json.dumps(data, default=str).encode())
        except BrokenPipeError:
            pass  # Client disconnected, ignore

    def read_json(self) -> dict:
        try:
            content_length = int(self.headers.get("Content-Length", 0))
            if content_length:
                body = self.rfile.read(content_length)
                return json.loads(body.decode())
        except Exception:
            pass
        return {}

    def do_GET(self):
        parsed = urlparse(self.path)

        if parsed.path == "/status":
            self.send_json(orchestrator.get_status())
        elif parsed.path == "/scenario/status":
            if orchestrator.scenario_engine:
                self.send_json(orchestrator.scenario_engine.get_status())
            else:
                self.send_json({"error": "no scenario loaded"}, 404)
        elif parsed.path == "/scenario/state":
            if orchestrator.scenario_engine:
                self.send_json(orchestrator.scenario_engine.get_entity_state())
            else:
                self.send_json({"error": "no scenario loaded"}, 404)
        elif parsed.path == "/scenario/metrics":
            if orchestrator.scenario_engine:
                self.send_json(orchestrator.scenario_engine.get_berth_metrics())
            else:
                self.send_json({"error": "no scenario loaded"}, 404)
        elif parsed.path == "/scenario/decisions":
            if orchestrator.scenario_engine:
                self.send_json(orchestrator.scenario_engine.get_decisions())
            else:
                self.send_json({"error": "no scenario loaded"}, 404)
        elif parsed.path == "/scenario/events":
            if orchestrator.scenario_engine:
                self.send_json(orchestrator.scenario_engine.get_event_log())
            else:
                self.send_json({"error": "no scenario loaded"}, 404)
        elif parsed.path == "/":
            self.send_dashboard()
        else:
            self.send_json({"error": "not found"}, 404)

    def do_POST(self):
        try:
            parsed = urlparse(self.path)
            data = self.read_json()

            if parsed.path == "/register":
                result = orchestrator.register(
                    data.get("node_id", "unknown"),
                    data.get("backend", "unknown"),
                    data.get("role", "unknown"),
                    data.get("equipment_type", ""),
                )
                self.send_json(result)

            elif parsed.path == "/ready":
                result = orchestrator.mark_ready(data.get("node_id", "unknown"))
                self.send_json(result)

            elif parsed.path == "/metrics":
                result = orchestrator.report_metrics(
                    data.get("node_id", "unknown"),
                    data.get("metrics", {}),
                )
                self.send_json(result)

            elif parsed.path == "/error":
                result = orchestrator.report_error(
                    data.get("node_id", "unknown"),
                    data.get("error", "unknown error"),
                )
                self.send_json(result)

            elif parsed.path == "/decide":
                result = orchestrator.request_decision(
                    data.get("node_id", "unknown"),
                    data.get("prompt", ""),
                    data.get("context"),
                )
                self.send_json(result)

            elif parsed.path == "/reset":
                orchestrator.reset()
                self.send_json({"status": "reset"})

            elif parsed.path == "/scenario/advance":
                if orchestrator.scenario_engine:
                    cycles = data.get("cycles", 1)
                    results = []
                    for _ in range(cycles):
                        fired = orchestrator.scenario_engine.advance_cycle()
                        results.extend(fired)
                    self.send_json({
                        "status": "advanced",
                        "cycle": orchestrator.scenario_engine.cycle,
                        "events_fired": len(results),
                    })
                else:
                    self.send_json({"error": "no scenario loaded"}, 404)

            elif parsed.path == "/scenario/start":
                if orchestrator.scenario_engine:
                    cycle_ms = data.get("cycle_duration_ms", 5000)
                    orchestrator.scenario_engine.start(cycle_ms)
                    self.send_json({"status": "started", "cycle_duration_ms": cycle_ms})
                else:
                    self.send_json({"error": "no scenario loaded"}, 404)

            elif parsed.path == "/scenario/stop":
                if orchestrator.scenario_engine:
                    orchestrator.scenario_engine.stop()
                    self.send_json({"status": "stopped", "cycle": orchestrator.scenario_engine.cycle})
                else:
                    self.send_json({"error": "no scenario loaded"}, 404)

            else:
                self.send_json({"error": "not found"}, 404)
        except BrokenPipeError:
            pass  # Client disconnected
        except Exception as e:
            print(f"POST error: {e}")

    def send_dashboard(self):
        status = orchestrator.get_status()
        scenario_html = ""

        if orchestrator.scenario_engine:
            sc = orchestrator.scenario_engine.get_status()
            phases_html = ""
            for phase in sc.get("phases", []):
                check = "&#10003;" if phase["complete"] else "&#9744;"
                phases_html += (
                    f"  {check} {phase['name']} (cycle {phase['trigger_cycle']}): "
                    f"{phase['fired_events']}/{phase['total_events']} events<br>"
                )

            metrics = orchestrator.scenario_engine.get_berth_metrics()
            metrics_html = ""
            if metrics:
                metrics_html = f"""
    <div class="box">
        <strong>Berth Metrics:</strong><br>
        Vessel: {metrics.get('vessel', '-')}<br>
        Active Cranes: {metrics.get('active_cranes', 0)} | Active Workers: {metrics.get('active_workers', 0)}<br>
        Throughput: {metrics.get('berth_throughput_moves_hr', 0)} moves/hr<br>
        Containers Remaining: {metrics.get('total_containers_remaining', 0)}<br>
    </div>"""

            scenario_html = f"""
    <div class="box" style="border-color: #ffaa00; color: #ffaa00;">
        <strong>Scenario:</strong> {sc.get('scenario', '-')}<br>
        <strong>Cycle:</strong> {sc.get('cycle', 0)} | {'RUNNING' if sc.get('running') else 'PAUSED'}<br>
        <strong>Events:</strong> {sc.get('fired_events', 0)} / {sc.get('total_events', 0)} ({sc.get('progress_pct', 0)}%)<br>
        <strong>Decisions:</strong> {sc.get('decisions_made', 0)}<br>
        <strong>Next:</strong> {sc.get('next_event_type', 'none')} at cycle {sc.get('next_event_cycle', '-')}<br>
        <div class="progress"><div class="progress-bar" style="width: {sc.get('progress_pct', 0)}%; background: #ffaa00;"></div></div>
    </div>

    <div class="box">
        <strong>Phases:</strong><br>
        {phases_html}
    </div>
    {metrics_html}
    <div class="box">
        <a href="/scenario/status">Scenario JSON</a> |
        <a href="/scenario/state">Entity State</a> |
        <a href="/scenario/metrics">Metrics</a> |
        <a href="/scenario/decisions">Decisions</a> |
        <a href="/scenario/events">Event Log</a>
    </div>"""

        # Simple ASCII dashboard
        html = f"""
<!DOCTYPE html>
<html>
<head>
    <title>Lab Orchestrator</title>
    <meta http-equiv="refresh" content="2">
    <style>
        body {{ font-family: monospace; background: #1a1a1a; color: #00ff00; padding: 20px; }}
        .box {{ border: 1px solid #00ff00; padding: 10px; margin: 10px 0; }}
        .progress {{ background: #333; height: 20px; }}
        .progress-bar {{ background: #00ff00; height: 100%; }}
        .error {{ color: #ff4444; }}
        h1 {{ border-bottom: 2px solid #00ff00; }}
    </style>
</head>
<body>
    <h1>Lab Orchestrator</h1>

    <div class="box">
        <strong>Test State:</strong> {status['test_state'].upper()}<br>
        <strong>Started:</strong> {status['test_start_time'] or 'Not started'}<br>
    </div>

    <div class="box">
        <strong>Deployment Progress:</strong> {status['registered']} / {status['expected_nodes']} ({status['progress_pct']}%)<br>
        <div class="progress"><div class="progress-bar" style="width: {status['progress_pct']}%"></div></div>
    </div>

    <div class="box">
        <strong>Ready Nodes:</strong> {status['ready']} / {status['expected_nodes']} ({status['ready_pct']}%)<br>
        <div class="progress"><div class="progress-bar" style="width: {status['ready_pct']}%"></div></div>
    </div>

    <div class="box">
        <strong>By Role:</strong><br>
        {'<br>'.join(f"  {role}: {counts['registered']} registered, {counts['ready']} ready" for role, counts in status['by_role'].items())}
    </div>

    <div class="box">
        <strong>Worker Roster:</strong><br>
        {'<br>'.join(f"  {w['worker_id']} ({w['display_name']}) — {w['role']} {'✓' if w['ready'] else '…'}" for w in status.get('workers', {{}}).values())}
    </div>

    <div class="box">
        <strong>By Backend:</strong><br>
        {'<br>'.join(f"  {backend}: {count}" for backend, count in status['by_backend'].items())}
    </div>

    <div class="box">
        <strong>LLM Provider:</strong> {status['llm_provider']}<br>
        <strong>Decisions:</strong> {status['llm_decisions']}<br>
        <strong>Avg Latency:</strong> {status['llm_avg_latency_ms']}ms<br>
        <strong>By Tier:</strong><br>
        {'<br>'.join(f"  {tier}: {count}" for tier, count in status['by_llm_tier'].items())}
    </div>

    <div class="box {'error' if status['total_errors'] else ''}">
        <strong>Errors:</strong> {status['total_errors']}<br>
        {('<br>'.join(status['nodes_with_errors'][:10])) if status['nodes_with_errors'] else 'None'}
    </div>
    {scenario_html}
    <div class="box">
        <a href="/status">JSON Status</a> |
        <form style="display:inline" method="POST" action="/reset"><button type="submit">Reset</button></form>
    </div>
</body>
</html>
"""
        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.end_headers()
        self.wfile.write(html.encode())


# Use ThreadingMixIn for concurrent request handling
from socketserver import ThreadingMixIn

class ThreadedHTTPServer(ThreadingMixIn, HTTPServer):
    """Handle requests in separate threads."""
    daemon_threads = True


def main():
    global orchestrator

    parser = argparse.ArgumentParser(description="Lab Orchestrator")
    parser.add_argument("--port", type=int, default=8080, help="HTTP port")
    parser.add_argument("--expected-nodes", type=int, default=447, help="Expected node count")
    parser.add_argument("--provider", type=str, default="dry-run",
                        choices=["dry-run", "ollama", "api"],
                        help="LLM provider for equipment agent decisions (default: dry-run)")
    parser.add_argument("--ollama-endpoint", type=str, default="http://localhost:11434",
                        help="Ollama API endpoint (default: http://localhost:11434)")
    parser.add_argument("--ollama-model", type=str, default="llama3:8b",
                        help="Ollama model name (default: llama3:8b)")
    parser.add_argument("--scenario", type=str, default=None,
                        help="Path to scenario JSON file (enables phase2-dry mode)")
    parser.add_argument("--auto-start", action="store_true",
                        help="Auto-start scenario cycle advancement")
    parser.add_argument("--cycle-duration-ms", type=int, default=5000,
                        help="Scenario cycle duration in milliseconds (default: 5000)")
    args = parser.parse_args()

    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")

    llm_provider = create_provider(
        args.provider,
        ollama_endpoint=args.ollama_endpoint,
        ollama_model=args.ollama_model,
    )
    orchestrator = Orchestrator(args.expected_nodes, llm_provider=llm_provider)

    if llm_provider.is_ready():
        print(f"LLM provider: {args.provider} (ready)")
    else:
        print(f"LLM provider: {args.provider} (not ready - check endpoint/model)")

    # Load scenario if specified
    if args.scenario:
        scenario_path = Path(args.scenario)
        if not scenario_path.is_absolute():
            scenario_path = Path.cwd() / scenario_path
        if not scenario_path.exists():
            print(f"Error: scenario file not found: {scenario_path}")
            sys.exit(1)

        engine = ScenarioEngine(str(scenario_path))
        engine.load()
        engine.init_modules()
        orchestrator.scenario_engine = engine

        if args.auto_start:
            engine.start(args.cycle_duration_ms)

    server = ThreadedHTTPServer(("0.0.0.0", args.port), OrchestratorHandler)
    print(f"Lab Orchestrator running on http://0.0.0.0:{args.port}")
    print(f"Expecting {args.expected_nodes} nodes")
    if orchestrator.scenario_engine:
        print(f"Scenario: {orchestrator.scenario_engine.scenario.get('name', 'unknown')}")
        print(f"Mode: phase2-dry")
    print(f"Dashboard: http://localhost:{args.port}/")
    print(f"Status API: http://localhost:{args.port}/status")

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...")
        if orchestrator.scenario_engine:
            orchestrator.scenario_engine.stop()
        server.shutdown()


if __name__ == "__main__":
    main()
