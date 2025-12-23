#!/usr/bin/env python3
"""
Lab Orchestrator - Simple HTTP service for coordinating containerlab simulations.

Nodes POST their status to this service. The orchestrator tracks:
- Node registration and readiness
- Sync/convergence status
- Errors and failures
- Test lifecycle (waiting -> ready -> running -> complete)

Usage:
    python3 orchestrator/main.py --expected-nodes 447 --port 8080

Endpoints:
    POST /register      - Node registers itself (called on startup)
    POST /ready         - Node reports it's ready (sync established)
    POST /metrics       - Node pushes metrics
    POST /error         - Node reports an error
    GET  /status        - Dashboard JSON
    GET  /              - Human-readable dashboard
    POST /reset         - Reset all state for new test run
"""

import argparse
import json
import threading
import time
from collections import defaultdict
from dataclasses import dataclass, field, asdict
from datetime import datetime
from http.server import HTTPServer, BaseHTTPRequestHandler
from typing import Optional
from urllib.parse import parse_qs, urlparse


@dataclass
class NodeState:
    node_id: str
    registered_at: str
    ready_at: Optional[str] = None
    last_seen: Optional[str] = None
    backend: str = "unknown"
    role: str = "unknown"
    errors: list = field(default_factory=list)
    metrics: dict = field(default_factory=dict)


class Orchestrator:
    def __init__(self, expected_nodes: int):
        self.expected_nodes = expected_nodes
        self.nodes: dict[str, NodeState] = {}
        self.test_start_time: Optional[str] = None
        self.test_state = "waiting"  # waiting -> deploying -> ready -> running -> complete
        self.lock = threading.Lock()

    def register(self, node_id: str, backend: str = "unknown", role: str = "unknown") -> dict:
        with self.lock:
            now = datetime.utcnow().isoformat()
            if node_id not in self.nodes:
                self.nodes[node_id] = NodeState(
                    node_id=node_id,
                    registered_at=now,
                    backend=backend,
                    role=role,
                )
            self.nodes[node_id].last_seen = now

            if self.test_state == "waiting":
                self.test_state = "deploying"
                self.test_start_time = now

            return {"status": "registered", "node_count": len(self.nodes)}

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
                "nodes_with_errors": [n.node_id for n in self.nodes.values() if n.errors],
            }

    def reset(self):
        with self.lock:
            self.nodes.clear()
            self.test_state = "waiting"
            self.test_start_time = None


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
        <strong>By Backend:</strong><br>
        {'<br>'.join(f"  {backend}: {count}" for backend, count in status['by_backend'].items())}
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
    args = parser.parse_args()

    orchestrator = Orchestrator(args.expected_nodes)

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
