#!/usr/bin/env python3
"""
peat-sim-orchestrator: Process-per-node orchestrator for Plan C (10K nodes).

Runs thousands of peat-sim processes directly (no Docker) on a single large VM.
Generates a full battalion hierarchy and launches each node as a separate OS process
with the correct environment variables for hierarchical mode TCP connectivity.
"""

import argparse
import logging
import os
import signal
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [orchestrator] %(levelname)s %(message)s",
    datefmt="%Y-%m-%dT%H:%M:%S",
)
log = logging.getLogger("orchestrator")


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------

@dataclass
class NodeSpec:
    """Specification for a single peat-sim node."""
    node_id: str
    role: str          # battalion_commander, company_commander, platoon_leader, squad_leader, soldier
    node_type: str     # same as role
    tier: str          # battalion, company, platoon, squad, soldier  (for launch ordering)
    port: int          # TCP_LISTEN port
    tcp_connect: str   # "127.0.0.1:PORT" or "" for battalion HQ
    company_id: str
    platoon_id: str
    squad_id: str
    squad_members: str  # comma-separated soldier node_ids (only for squad leaders)


@dataclass
class ManagedProcess:
    """A running peat-sim process."""
    node_id: str
    tier: str
    proc: subprocess.Popen
    log_file: "open"  # file handle kept open for the lifetime of the process
    started_at: float = field(default_factory=time.monotonic)


# ---------------------------------------------------------------------------
# Hierarchy generation
# ---------------------------------------------------------------------------

def generate_hierarchy(
    num_companies: int,
    platoons_per_company: int,
    squads_per_platoon: int,
    soldiers_per_squad: int,
    base_port: int,
) -> List[NodeSpec]:
    """Build the full node list with port assignments and TCP wiring."""

    nodes: List[NodeSpec] = []
    port_counter = base_port

    def next_port() -> int:
        nonlocal port_counter
        p = port_counter
        port_counter += 1
        return p

    # Battalion HQ
    bn_port = next_port()
    nodes.append(NodeSpec(
        node_id="battalion-hq",
        role="battalion_commander",
        node_type="battalion_commander",
        tier="battalion",
        port=bn_port,
        tcp_connect="",
        company_id="",
        platoon_id="",
        squad_id="",
        squad_members="",
    ))

    for c in range(1, num_companies + 1):
        company_id = f"company-{c}"

        # Company commander
        cc_port = next_port()
        nodes.append(NodeSpec(
            node_id=f"company-{c}-commander",
            role="company_commander",
            node_type="company_commander",
            tier="company",
            port=cc_port,
            tcp_connect=f"127.0.0.1:{bn_port}",
            company_id=company_id,
            platoon_id="",
            squad_id="",
            squad_members="",
        ))

        for p in range(1, platoons_per_company + 1):
            platoon_id = f"company-{c}-platoon-{p}"

            # Platoon leader
            pl_port = next_port()
            nodes.append(NodeSpec(
                node_id=f"company-{c}-platoon-{p}-leader",
                role="platoon_leader",
                node_type="platoon_leader",
                tier="platoon",
                port=pl_port,
                tcp_connect=f"127.0.0.1:{cc_port}",
                company_id=company_id,
                platoon_id=platoon_id,
                squad_id="",
                squad_members="",
            ))

            for s in range(1, squads_per_platoon + 1):
                squad_id = f"company-{c}-platoon-{p}-squad-{s}"

                # Soldier node IDs for SQUAD_MEMBERS
                soldier_ids = [
                    f"company-{c}-platoon-{p}-squad-{s}-soldier-{i}"
                    for i in range(1, soldiers_per_squad + 1)
                ]

                # Squad leader
                sl_port = next_port()
                nodes.append(NodeSpec(
                    node_id=f"company-{c}-platoon-{p}-squad-{s}-leader",
                    role="squad_leader",
                    node_type="squad_leader",
                    tier="squad",
                    port=sl_port,
                    tcp_connect=f"127.0.0.1:{pl_port}",
                    company_id=company_id,
                    platoon_id=platoon_id,
                    squad_id=squad_id,
                    squad_members=",".join(soldier_ids),
                ))

                # Soldiers
                for i in range(1, soldiers_per_squad + 1):
                    sol_port = next_port()
                    nodes.append(NodeSpec(
                        node_id=f"company-{c}-platoon-{p}-squad-{s}-soldier-{i}",
                        role="soldier",
                        node_type="soldier",
                        tier="soldier",
                        port=sol_port,
                        tcp_connect=f"127.0.0.1:{sl_port}",
                        company_id=company_id,
                        platoon_id=platoon_id,
                        squad_id=squad_id,
                        squad_members="",
                    ))

    return nodes


def count_by_tier(nodes: List[NodeSpec]) -> Dict[str, int]:
    counts: Dict[str, int] = {}
    for n in nodes:
        counts[n.tier] = counts.get(n.tier, 0) + 1
    return counts


# ---------------------------------------------------------------------------
# Environment builder
# ---------------------------------------------------------------------------

def build_env(node: NodeSpec, backend: str) -> Dict[str, str]:
    """Build the environment-variable dict for a single peat-sim process."""
    env: Dict[str, str] = {
        "NODE_ID": node.node_id,
        "ROLE": node.role,
        "NODE_TYPE": node.node_type,
        "MODE": "hierarchical",
        "BACKEND": backend,
        "UPDATE_RATE_MS": "5000",
        "TCP_LISTEN": str(node.port),
        "CIRCUIT_FAILURE_THRESHOLD": "3",
        "CIRCUIT_FAILURE_WINDOW_SECS": "2",
        "CIRCUIT_OPEN_TIMEOUT_SECS": "2",
        "CIRCUIT_SUCCESS_THRESHOLD": "2",
    }

    if node.tcp_connect:
        env["TCP_CONNECT"] = node.tcp_connect

    if node.company_id:
        env["COMPANY_ID"] = node.company_id
    if node.platoon_id:
        env["PLATOON_ID"] = node.platoon_id
    if node.squad_id:
        env["SQUAD_ID"] = node.squad_id
    if node.squad_members:
        env["SQUAD_MEMBERS"] = node.squad_members

    return env


# ---------------------------------------------------------------------------
# Process launcher
# ---------------------------------------------------------------------------

class Orchestrator:
    """Manages the lifecycle of all peat-sim processes."""

    def __init__(
        self,
        binary: str,
        backend: str,
        log_dir: Path,
        batch_size: int,
        batch_delay_secs: float,
        duration_secs: int,
    ) -> None:
        self.binary = binary
        self.backend = backend
        self.log_dir = log_dir
        self.batch_size = batch_size
        self.batch_delay_secs = batch_delay_secs
        self.duration_secs = duration_secs

        self.processes: List[ManagedProcess] = []
        self._shutting_down = False

    # -- launching ----------------------------------------------------------

    def _launch_node(self, node: NodeSpec) -> ManagedProcess:
        """Spawn a single peat-sim process."""
        env = {**os.environ, **build_env(node, self.backend)}

        # Build CLI args matching entrypoint.sh for hierarchical mode
        args = [
            self.binary,
            "--node-id", node.node_id,
            "--mode", "hierarchical",
            "--backend", self.backend,
            "--node-type", node.node_type,
            "--update-rate-ms", "5000",
            "--tcp-listen", str(node.port),
        ]
        if node.tcp_connect:
            args.extend(["--tcp-connect", node.tcp_connect])

        log_path = self.log_dir / f"{node.node_id}.log"
        log_fh = open(log_path, "w")

        proc = subprocess.Popen(
            args,
            env=env,
            stdout=log_fh,
            stderr=subprocess.STDOUT,
            # Ensure children get their own process group so we can signal them
            preexec_fn=os.setpgrp if sys.platform != "win32" else None,
        )

        mp = ManagedProcess(
            node_id=node.node_id,
            tier=node.tier,
            proc=proc,
            log_file=log_fh,
        )
        self.processes.append(mp)
        return mp

    def _launch_tier(self, nodes: List[NodeSpec], tier_name: str) -> None:
        """Launch all nodes in a tier, respecting batch limits."""
        total = len(nodes)
        if total == 0:
            return

        log.info("Launching tier %-10s: %d nodes (batch_size=%d, delay=%.1fs)",
                 tier_name, total, self.batch_size, self.batch_delay_secs)

        launched = 0
        for i, node in enumerate(nodes):
            if self._shutting_down:
                log.warning("Shutdown requested during launch, aborting tier %s", tier_name)
                return

            self._launch_node(node)
            launched += 1

            # Batch gating: pause between batches (but not after the last node)
            if launched % self.batch_size == 0 and i < total - 1:
                log.info("  ... launched %d/%d in tier %s, pausing %.1fs",
                         launched, total, tier_name, self.batch_delay_secs)
                time.sleep(self.batch_delay_secs)

        log.info("  Tier %s: all %d launched", tier_name, launched)

    def launch_all(self, nodes: List[NodeSpec]) -> None:
        """Launch every node in tier order: battalion -> company -> platoon -> squad -> soldier."""
        tier_order = ["battalion", "company", "platoon", "squad", "soldier"]
        tier_map: Dict[str, List[NodeSpec]] = {t: [] for t in tier_order}
        for n in nodes:
            tier_map[n.tier].append(n)

        for tier in tier_order:
            if self._shutting_down:
                break
            self._launch_tier(tier_map[tier], tier)
            # Small pause between tiers so listeners are ready before connectors
            if tier != tier_order[-1] and not self._shutting_down:
                time.sleep(1.0)

    # -- health monitoring --------------------------------------------------

    def health_check(self) -> Dict[str, int]:
        """Poll all processes and return summary counts."""
        running = 0
        failed = 0
        for mp in self.processes:
            rc = mp.proc.poll()
            if rc is None:
                running += 1
            else:
                failed += 1
        return {"running": running, "failed": failed, "total": len(self.processes)}

    def get_rss_bytes(self) -> int:
        """Estimate total RSS usage across all running child processes."""
        total_rss = 0
        for mp in self.processes:
            if mp.proc.poll() is not None:
                continue
            try:
                statm_path = f"/proc/{mp.proc.pid}/statm"
                with open(statm_path) as f:
                    # statm fields: size resident shared text lib data dt (in pages)
                    parts = f.read().split()
                    resident_pages = int(parts[1])
                    total_rss += resident_pages * os.sysconf("SC_PAGE_SIZE")
            except (FileNotFoundError, IndexError, ValueError, PermissionError):
                pass
        return total_rss

    def print_status(self) -> None:
        """Print a summary status line."""
        stats = self.health_check()
        rss = self.get_rss_bytes()
        rss_gb = rss / (1024 ** 3)
        log.info(
            "STATUS: total=%d running=%d failed=%d RSS=%.2f GB",
            stats["total"], stats["running"], stats["failed"], rss_gb,
        )

    # -- monitoring loop ----------------------------------------------------

    def run_monitoring_loop(self, poll_interval: float = 10.0) -> None:
        """Run until duration expires or all processes die."""
        start = time.monotonic()
        deadline = start + self.duration_secs

        log.info("Monitoring loop started. Duration: %ds, poll interval: %.0fs",
                 self.duration_secs, poll_interval)

        while not self._shutting_down:
            now = time.monotonic()
            if now >= deadline:
                log.info("Duration of %ds reached. Shutting down.", self.duration_secs)
                break

            self.print_status()
            stats = self.health_check()
            if stats["running"] == 0:
                log.warning("All processes have exited.")
                break

            # Sleep in small increments so we can respond to signals quickly
            sleep_until = min(now + poll_interval, deadline)
            while time.monotonic() < sleep_until and not self._shutting_down:
                time.sleep(0.5)

    # -- shutdown -----------------------------------------------------------

    def shutdown(self) -> None:
        """Gracefully terminate all running processes."""
        if self._shutting_down:
            return
        self._shutting_down = True

        running = [mp for mp in self.processes if mp.proc.poll() is None]
        if not running:
            log.info("No running processes to shut down.")
            return

        log.info("Sending SIGTERM to %d running processes...", len(running))
        for mp in running:
            try:
                mp.proc.terminate()
            except OSError:
                pass

        # Wait up to 5 seconds for graceful exit
        grace_deadline = time.monotonic() + 5.0
        while time.monotonic() < grace_deadline:
            still_alive = [mp for mp in running if mp.proc.poll() is None]
            if not still_alive:
                log.info("All processes exited gracefully.")
                break
            time.sleep(0.25)
        else:
            still_alive = [mp for mp in running if mp.proc.poll() is None]
            if still_alive:
                log.warning("Sending SIGKILL to %d remaining processes...", len(still_alive))
                for mp in still_alive:
                    try:
                        mp.proc.kill()
                    except OSError:
                        pass
                # Final wait
                for mp in still_alive:
                    try:
                        mp.proc.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        pass

        # Close all log file handles
        for mp in self.processes:
            try:
                mp.log_file.close()
            except Exception:
                pass

        # Final status
        self.print_status()


# ---------------------------------------------------------------------------
# Signal handling
# ---------------------------------------------------------------------------

_orchestrator_ref: Optional[Orchestrator] = None


def _signal_handler(signum: int, frame: object) -> None:
    sig_name = signal.Signals(signum).name
    log.info("Received %s, initiating shutdown...", sig_name)
    if _orchestrator_ref is not None:
        _orchestrator_ref.shutdown()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def parse_args(argv: Optional[List[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="peat-sim process-per-node orchestrator (Plan C)",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--companies", type=int, default=67,
                        help="Number of companies in the battalion")
    parser.add_argument("--platoons-per-company", type=int, default=4,
                        help="Platoons per company")
    parser.add_argument("--squads-per-platoon", type=int, default=4,
                        help="Squads per platoon")
    parser.add_argument("--soldiers-per-squad", type=int, default=8,
                        help="Soldiers per squad")
    parser.add_argument("--binary", type=str, required=True,
                        help="Path to the peat-sim binary")
    parser.add_argument("--base-port", type=int, default=10000,
                        help="Starting port number (increments from here)")
    parser.add_argument("--batch-size", type=int, default=200,
                        help="Number of processes to launch per batch")
    parser.add_argument("--batch-delay-secs", type=float, default=3.0,
                        help="Seconds to wait between batches")
    parser.add_argument("--backend", type=str, default="automerge",
                        help="Sync backend (automerge or ditto)")
    parser.add_argument("--log-dir", type=str, default="./logs",
                        help="Directory for per-node log files")
    parser.add_argument("--duration-secs", type=int, default=300,
                        help="How long to run the simulation (seconds)")
    return parser.parse_args(argv)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main(argv: Optional[List[str]] = None) -> None:
    global _orchestrator_ref

    args = parse_args(argv)

    # Validate binary exists
    binary_path = Path(args.binary).resolve()
    if not binary_path.is_file():
        log.error("Binary not found: %s", binary_path)
        sys.exit(1)
    if not os.access(binary_path, os.X_OK):
        log.error("Binary is not executable: %s", binary_path)
        sys.exit(1)

    # Create log directory
    log_dir = Path(args.log_dir).resolve()
    log_dir.mkdir(parents=True, exist_ok=True)
    log.info("Log directory: %s", log_dir)

    # Generate hierarchy
    log.info(
        "Generating hierarchy: %d companies x %d platoons x %d squads x %d soldiers",
        args.companies, args.platoons_per_company,
        args.squads_per_platoon, args.soldiers_per_squad,
    )
    nodes = generate_hierarchy(
        num_companies=args.companies,
        platoons_per_company=args.platoons_per_company,
        squads_per_platoon=args.squads_per_platoon,
        soldiers_per_squad=args.soldiers_per_squad,
        base_port=args.base_port,
    )

    tier_counts = count_by_tier(nodes)
    log.info("Total nodes: %d", len(nodes))
    for tier, count in sorted(tier_counts.items()):
        log.info("  %-12s %d", tier, count)

    port_range_end = args.base_port + len(nodes) - 1
    log.info("Port range: %d - %d", args.base_port, port_range_end)

    # Create orchestrator
    orch = Orchestrator(
        binary=str(binary_path),
        backend=args.backend,
        log_dir=log_dir,
        batch_size=args.batch_size,
        batch_delay_secs=args.batch_delay_secs,
        duration_secs=args.duration_secs,
    )
    _orchestrator_ref = orch

    # Install signal handlers
    signal.signal(signal.SIGINT, _signal_handler)
    signal.signal(signal.SIGTERM, _signal_handler)

    # Launch
    log.info("Starting launch sequence with binary: %s", binary_path)
    launch_start = time.monotonic()
    orch.launch_all(nodes)
    launch_elapsed = time.monotonic() - launch_start
    log.info("Launch complete in %.1f seconds", launch_elapsed)

    # Run monitoring loop
    orch.run_monitoring_loop()

    # Shutdown
    orch.shutdown()

    # Print final summary
    stats = orch.health_check()
    log.info("=" * 60)
    log.info("FINAL SUMMARY")
    log.info("  Total processes spawned: %d", stats["total"])
    log.info("  Still running at exit:   %d", stats["running"])
    log.info("  Failed/exited:           %d", stats["failed"])
    log.info("  Launch time:             %.1fs", launch_elapsed)
    log.info("  Log directory:           %s", log_dir)
    log.info("=" * 60)


if __name__ == "__main__":
    main()
