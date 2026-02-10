#!/usr/bin/env python3
"""
Lab Orchestrator - Simple HTTP service for coordinating containerlab simulations.

Nodes POST their status to this service. The orchestrator tracks:
- Node registration and readiness
- Sync/convergence status
- Errors and failures
- Test lifecycle (waiting -> ready -> running -> complete)
- Equipment agent LLM decisions (via tiered provider system)

Usage:
    python3 orchestrator/main.py --expected-nodes 447 --port 8080
    python3 orchestrator/main.py --expected-nodes 12 --provider ollama --ollama-model llama3:8b

Endpoints:
    POST /register      - Node registers itself (called on startup)
    POST /ready         - Node reports it's ready (sync established)
    POST /metrics       - Node pushes metrics
    POST /error         - Node reports an error
    POST /decide        - Equipment agent requests LLM decision
    GET  /status        - Dashboard JSON
    GET  /              - Human-readable dashboard
    POST /reset         - Reset all state for new test run
"""

import argparse
import json
import logging
import threading
import time
from collections import defaultdict
from dataclasses import dataclass, field, asdict
from datetime import datetime
from http.server import HTTPServer, BaseHTTPRequestHandler
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

            return {
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

    def reset(self):
        with self.lock:
            self.nodes.clear()
            self.test_state = "waiting"
            self.test_start_time = None
            self.llm_decision_count = 0
            self.llm_total_latency_ms = 0.0


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

    def send_json(self, data: dict, status: int = 200):
        try:
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(json.dumps(data).encode())
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

            else:
                self.send_json({"error": "not found"}, 404)
        except BrokenPipeError:
            pass  # Client disconnected
        except Exception as e:
            print(f"POST error: {e}")

    def send_dashboard(self):
        status = orchestrator.get_status()

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

    server = ThreadedHTTPServer(("0.0.0.0", args.port), OrchestratorHandler)
    print(f"Lab Orchestrator running on http://0.0.0.0:{args.port}")
    print(f"Expecting {args.expected_nodes} nodes")
    print(f"Dashboard: http://localhost:{args.port}/")
    print(f"Status API: http://localhost:{args.port}/status")

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...")
        server.shutdown()


if __name__ == "__main__":
    main()
