"""
Live Dashboard and Post-Run Summary for Port Operations Simulation

Renders real-time OODA loop state to the terminal using ANSI escape codes.
No external dependencies — works in any terminal with color support.

Dashboard updates after each OODA phase (observe, decide, act).
Post-run summary dumps all HIVE documents and the complete event log.
"""

from __future__ import annotations

import json
import os
import sys
import time
from dataclasses import dataclass, field
from typing import Any, Optional


# ─── ANSI helpers ───────────────────────────────────────────────────────────

RESET = "\033[0m"
BOLD = "\033[1m"
DIM = "\033[2m"
CYAN = "\033[36m"
GREEN = "\033[32m"
YELLOW = "\033[33m"
RED = "\033[31m"
MAGENTA = "\033[35m"
WHITE = "\033[37m"
BG_BLUE = "\033[44m"
CLEAR_SCREEN = "\033[2J\033[H"
HIDE_CURSOR = "\033[?25l"
SHOW_CURSOR = "\033[?25h"


def _bar(done: int, total: int, width: int = 30) -> str:
    """Render a progress bar: [████████░░░░░░░░░░]"""
    if total == 0:
        return f"[{'░' * width}] 0/0"
    pct = done / total
    filled = int(width * pct)
    empty = width - filled
    color = GREEN if pct >= 0.8 else YELLOW if pct >= 0.4 else CYAN
    return f"{color}[{'█' * filled}{'░' * empty}]{RESET} {done}/{total} ({pct:.0%})"


def _trunc(text: str, max_len: int = 72) -> str:
    """Truncate text with ellipsis."""
    if len(text) <= max_len:
        return text
    return text[:max_len - 1] + "…"


def _box_top(width: int) -> str:
    return f"{CYAN}╔{'═' * width}╗{RESET}"


def _box_mid(width: int) -> str:
    return f"{CYAN}╠{'═' * width}╣{RESET}"


def _box_bot(width: int) -> str:
    return f"{CYAN}╚{'═' * width}╝{RESET}"


def _box_row(text: str, width: int) -> str:
    # Strip ANSI for padding calculation
    import re
    plain = re.sub(r'\033\[[0-9;]*m', '', text)
    pad = max(0, width - len(plain))
    return f"{CYAN}║{RESET} {text}{' ' * pad}{CYAN}║{RESET}"


def _section(title: str) -> str:
    return f"  {BOLD}{MAGENTA}▌ {title} ▌{RESET}"


# ─── Data structures ───────────────────────────────────────────────────────

@dataclass
class DashboardState:
    """Accumulated state for dashboard rendering."""
    node_id: str = ""
    sim_time: str = "T+00:00"
    cycle: int = 0
    max_cycles: int = 0

    # From observe
    capabilities: dict = field(default_factory=dict)
    team_state: dict = field(default_factory=dict)
    container_queue: dict = field(default_factory=dict)
    tasking: dict = field(default_factory=dict)

    # From decide
    last_action: str = ""
    last_arguments: dict = field(default_factory=dict)
    last_reasoning: str = ""

    # From act
    last_result: str = ""
    last_observe_ms: float = 0
    last_decide_ms: float = 0
    last_act_ms: float = 0

    # Accumulated
    events: list = field(default_factory=list)
    total_actions: int = 0
    total_waits: int = 0
    total_errors: int = 0
    all_metrics: list = field(default_factory=list)

    # HIVE documents (captured from debug dump)
    hive_documents: dict = field(default_factory=dict)
    hive_events: list = field(default_factory=list)


# ─── Live Dashboard ────────────────────────────────────────────────────────

class LiveDashboard:
    """
    Real-time terminal dashboard for the OODA loop.

    Call update_observe(), update_decide(), update_act() after each phase.
    The dashboard redraws the full screen each time.
    """

    def __init__(self, node_id: str, max_cycles: int, enabled: bool = True):
        self.state = DashboardState(node_id=node_id, max_cycles=max_cycles)
        self.enabled = enabled
        try:
            self._width = min(os.get_terminal_size().columns - 2, 76) if enabled else 76
        except OSError:
            self._width = 76
        self._inner = self._width - 2  # inside the box
        if enabled:
            sys.stderr.write(HIDE_CURSOR)
            sys.stderr.flush()

    def cleanup(self):
        """Restore terminal state."""
        if self.enabled:
            sys.stderr.write(SHOW_CURSOR)
            sys.stderr.flush()

    def update_observe(self, observed_state: dict, sim_time: str, cycle: int):
        """Called after OBSERVE phase with full MCP resource state."""
        self.state.sim_time = sim_time
        self.state.cycle = cycle
        self.state.capabilities = observed_state.get("my_capabilities", {})
        self.state.team_state = observed_state.get("team_state", {})
        self.state.container_queue = observed_state.get("container_queue", {})
        self.state.tasking = observed_state.get("tasking", {})

    def update_decide(self, action: str, arguments: dict, reasoning: str, decide_ms: float):
        """Called after DECIDE phase with LLM decision."""
        self.state.last_action = action
        self.state.last_arguments = arguments
        self.state.last_reasoning = reasoning
        self.state.last_decide_ms = decide_ms

    def update_act(self, result: str, observe_ms: float, decide_ms: float, act_ms: float, success: bool):
        """Called after ACT phase — triggers a full redraw."""
        self.state.last_result = result
        self.state.last_observe_ms = observe_ms
        self.state.last_decide_ms = decide_ms
        self.state.last_act_ms = act_ms

        if self.state.last_action == "wait":
            self.state.total_waits += 1
        elif "Error" in result:
            self.state.total_errors += 1
        else:
            self.state.total_actions += 1

        # Track event
        self.state.events.append({
            "sim_time": self.state.sim_time,
            "action": self.state.last_action,
            "args": self.state.last_arguments,
            "result": result[:120],
            "success": success,
        })

        self.state.all_metrics.append({
            "cycle": self.state.cycle,
            "observe_ms": observe_ms,
            "decide_ms": decide_ms,
            "act_ms": act_ms,
        })

        if self.enabled:
            self._render()

    def update_hive_dump(self, documents: dict, events: list):
        """Called with full HIVE state dump (for post-run summary)."""
        self.state.hive_documents = documents
        self.state.hive_events = events

    def _render(self):
        """Full screen redraw."""
        s = self.state
        w = self._inner
        lines = []

        # Header
        lines.append(_box_top(w + 1))

        status = s.capabilities.get("operational_status", "UNKNOWN")
        status_color = GREEN if status == "OPERATIONAL" else YELLOW if status == "DEGRADED" else RED
        header = f"  {BOLD}HIVE Port Operations{RESET} — {CYAN}{s.node_id}{RESET} @ {s.capabilities.get('assignment', {}).get('berth', '?')}"
        lines.append(_box_row(header, w))

        sub = f"  Sim: {BOLD}{s.sim_time}{RESET}  │  Cycle: {s.cycle}/{s.max_cycles}  │  Status: {status_color}{status}{RESET}"
        lines.append(_box_row(sub, w))

        lines.append(_box_mid(w + 1))

        # Container Queue
        lines.append(_box_row(_section("CONTAINER QUEUE"), w))
        queue = s.container_queue
        completed = queue.get("completed_count", 0)
        total = queue.get("total_containers", 0)
        lines.append(_box_row(f"  {_bar(completed, total)}", w))

        next_containers = queue.get("next_containers", [])
        if next_containers:
            nc = next_containers[0]
            hazmat_flag = f"  {RED}⚠ HAZMAT CLASS {nc.get('hazmat_class')}{RESET}" if nc.get("hazmat") else ""
            lines.append(_box_row(f"  Next: {BOLD}{nc.get('container_id', '?')}{RESET} ({nc.get('weight_tons', '?')}t, {nc.get('destination_block', '?')}){hazmat_flag}", w))
        else:
            lines.append(_box_row(f"  {DIM}Queue empty — all containers processed{RESET}", w))

        hazmat_remaining = sum(1 for c in next_containers if c.get("hazmat") and c.get("status") != "COMPLETED")
        if hazmat_remaining:
            lines.append(_box_row(f"  {YELLOW}Hazmat in window: {hazmat_remaining}{RESET}", w))
        lines.append(_box_row("", w))

        # Last Action
        lines.append(_box_row(_section("LAST ACTION"), w))
        if s.last_action:
            action_color = GREEN if s.last_action == "complete_container_move" else YELLOW if s.last_action == "request_support" else CYAN
            args_str = ", ".join(f"{k}={v}" for k, v in s.last_arguments.items())
            lines.append(_box_row(f"  {action_color}{s.last_action}{RESET}({_trunc(args_str, 50)})", w))
            lines.append(_box_row(f"  → {_trunc(s.last_result, w - 6)}", w))
            if s.last_reasoning:
                lines.append(_box_row(f"  {DIM}Reasoning: {_trunc(s.last_reasoning, w - 16)}{RESET}", w))
        else:
            lines.append(_box_row(f"  {DIM}No action yet{RESET}", w))
        lines.append(_box_row("", w))

        # Crane Capabilities
        lines.append(_box_row(_section("CRANE CAPABILITIES"), w))
        caps = s.capabilities.get("capabilities", {})
        lift = caps.get("container_lift", {})
        hazmat = caps.get("hazmat_rated", {})
        if lift:
            lift_status = lift.get("status", "?")
            lift_color = GREEN if lift_status == "READY" else YELLOW
            lines.append(_box_row(
                f"  CONTAINER_LIFT: {lift_color}{lift_status}{RESET}  │  "
                f"{lift.get('lift_capacity_tons', '?')}t cap  │  "
                f"{lift.get('moves_per_hour', '?')} moves/hr",
                w
            ))
        if hazmat:
            cert = hazmat.get("certification_valid", False)
            cert_color = GREEN if cert else RED
            lines.append(_box_row(
                f"  HAZMAT_RATED: {cert_color}{'VALID' if cert else 'EXPIRED'}{RESET}  │  "
                f"Classes {hazmat.get('classes', [])}",
                w
            ))
        health = s.capabilities.get("equipment_health", {})
        if health:
            hyd = health.get("hydraulic_pct", "?")
            hyd_color = GREEN if isinstance(hyd, (int, float)) and hyd >= 70 else YELLOW if isinstance(hyd, (int, float)) and hyd >= 50 else RED
            lines.append(_box_row(
                f"  Health: {hyd_color}Hydraulic {hyd}%{RESET}  │  "
                f"Spreader {health.get('spreader_alignment', '?')}  │  "
                f"Electrical {health.get('electrical_status', '?')}",
                w
            ))

        # Metrics
        metrics = s.capabilities.get("metrics", {})
        if metrics:
            lines.append(_box_row(
                f"  Moves: {BOLD}{metrics.get('moves_completed', 0)}{RESET}  │  "
                f"Tonnage: {BOLD}{metrics.get('total_tons_lifted', 0):.1f}t{RESET}",
                w
            ))
        lines.append(_box_row("", w))

        # Team State
        lines.append(_box_row(_section("TEAM STATE"), w))
        team = s.team_state
        members = team.get("team_members", {})
        if members:
            member_strs = []
            for mid, mdata in members.items():
                mstatus = mdata.get("status", "?")
                mcolor = GREEN if mstatus == "OPERATIONAL" else YELLOW
                member_strs.append(f"{mid}({mcolor}{mstatus}{RESET})")
            lines.append(_box_row(f"  Members: {', '.join(member_strs)}", w))
        team_moves = team.get("moves_completed", 0)
        team_remaining = team.get("moves_remaining", 0)
        gaps = team.get("gap_analysis", [])
        lines.append(_box_row(
            f"  Team moves: {team_moves}  │  Remaining: {team_remaining}  │  "
            f"Gaps: {RED if gaps else DIM}{len(gaps)}{RESET}",
            w
        ))
        if gaps:
            latest_gap = gaps[-1]
            lines.append(_box_row(
                f"  {YELLOW}Latest gap: {latest_gap.get('capability', '?')} — {_trunc(latest_gap.get('reason', ''), 45)}{RESET}",
                w
            ))
        lines.append(_box_row("", w))

        # Recent Events
        lines.append(_box_row(_section("RECENT EVENTS"), w))
        recent = s.events[-5:] if s.events else []
        for ev in reversed(recent):
            ev_color = GREEN if ev["success"] else RED
            action_short = ev["action"]
            args_short = ""
            if ev["args"]:
                first_val = next(iter(ev["args"].values()), "")
                args_short = f"  {str(first_val)[:30]}"
            lines.append(_box_row(
                f"  {DIM}{ev['sim_time']}{RESET}  {ev_color}{action_short}{RESET}{args_short}",
                w
            ))
        if not recent:
            lines.append(_box_row(f"  {DIM}No events yet{RESET}", w))
        lines.append(_box_row("", w))

        # Cycle Metrics
        lines.append(_box_row(_section("CYCLE METRICS"), w))
        total_ms = s.last_observe_ms + s.last_decide_ms + s.last_act_ms
        lines.append(_box_row(
            f"  Observe: {s.last_observe_ms:.1f}ms  │  "
            f"Decide: {s.last_decide_ms:.1f}ms  │  "
            f"Act: {s.last_act_ms:.1f}ms  │  "
            f"Total: {BOLD}{total_ms:.1f}ms{RESET}",
            w
        ))
        if s.all_metrics:
            avg_total = sum(m["observe_ms"] + m["decide_ms"] + m["act_ms"] for m in s.all_metrics) / len(s.all_metrics)
            lines.append(_box_row(
                f"  Avg: {avg_total:.1f}ms/cycle  │  "
                f"Actions: {GREEN}{s.total_actions}{RESET}  │  "
                f"Waits: {DIM}{s.total_waits}{RESET}  │  "
                f"Errors: {RED if s.total_errors else DIM}{s.total_errors}{RESET}",
                w
            ))

        lines.append(_box_bot(w + 1))

        # Write to stderr (stdout reserved for METRICS JSON)
        output = CLEAR_SCREEN + "\n".join(lines) + "\n"
        sys.stderr.write(output)
        sys.stderr.flush()


# ─── Post-Run Summary ──────────────────────────────────────────────────────

def render_post_run_summary(state: DashboardState) -> str:
    """Generate a complete post-run summary with HIVE document dumps."""
    lines = []
    s = state

    sep = f"{CYAN}{'═' * 60}{RESET}"

    lines.append("")
    lines.append(sep)
    lines.append(f"  {BOLD}POST-RUN SUMMARY{RESET} — {CYAN}{s.node_id}{RESET}")
    lines.append(sep)

    # Simulation stats
    lines.append("")
    lines.append(f"  {BOLD}{MAGENTA}SIMULATION{RESET}")
    lines.append(f"    Duration: T+00:00 → {s.sim_time}")
    lines.append(f"    Cycles: {s.cycle}")
    lines.append(f"    Actions: {GREEN}{s.total_actions}{RESET}  │  "
                 f"Waits: {s.total_waits}  │  Errors: {RED if s.total_errors else DIM}{s.total_errors}{RESET}")

    # Queue results
    queue = s.container_queue
    completed = queue.get("completed_count", 0)
    total = queue.get("total_containers", 0)
    lines.append("")
    lines.append(f"  {BOLD}{MAGENTA}QUEUE RESULTS{RESET}")
    lines.append(f"    Completed: {completed}/{total} containers ({completed/total*100:.0f}%)" if total else "    No containers")

    metrics = s.capabilities.get("metrics", {})
    if metrics:
        lines.append(f"    Total tonnage: {metrics.get('total_tons_lifted', 0):.1f}t lifted")
        lines.append(f"    Moves completed: {metrics.get('moves_completed', 0)}")

    gaps = s.team_state.get("gap_analysis", [])
    if gaps:
        lines.append(f"    Hazmat escalations: {len(gaps)}")

    # Performance
    lines.append("")
    lines.append(f"  {BOLD}{MAGENTA}PERFORMANCE{RESET}")
    if s.all_metrics:
        avg_obs = sum(m["observe_ms"] for m in s.all_metrics) / len(s.all_metrics)
        avg_dec = sum(m["decide_ms"] for m in s.all_metrics) / len(s.all_metrics)
        avg_act = sum(m["act_ms"] for m in s.all_metrics) / len(s.all_metrics)
        avg_total = avg_obs + avg_dec + avg_act
        lines.append(f"    Avg cycle: {avg_total:.1f}ms (observe: {avg_obs:.1f}ms, decide: {avg_dec:.1f}ms, act: {avg_act:.1f}ms)")
        lines.append(f"    Success rate: {GREEN}{(s.total_actions + s.total_waits)}/{s.cycle} ({(s.total_actions + s.total_waits)/s.cycle*100:.0f}%){RESET}" if s.cycle else "")

    # HIVE Documents
    lines.append("")
    lines.append(sep)
    lines.append(f"  {BOLD}HIVE DOCUMENTS{RESET}")
    lines.append(sep)

    docs = s.hive_documents
    if docs:
        for collection, col_docs in docs.items():
            for doc_id, doc_data in col_docs.items():
                lines.append("")
                lines.append(f"  {CYAN}[{collection}/{doc_id}]{RESET}")
                _render_dict(lines, doc_data, indent=4, max_depth=4)
    else:
        lines.append(f"  {DIM}No HIVE documents captured (add --dashboard to capture){RESET}")

    # Event Log
    lines.append("")
    lines.append(sep)
    lines.append(f"  {BOLD}EVENT LOG{RESET} ({len(s.hive_events)} events)")
    lines.append(sep)

    if s.hive_events:
        for i, ev in enumerate(s.hive_events, 1):
            ev_type = ev.get("event_type", "?")
            source = ev.get("source", "?")
            priority = ev.get("priority", "NORMAL")
            agg = ev.get("aggregation_policy", "?")
            ts = ev.get("timestamp_us", 0)

            priority_color = RED if priority == "CRITICAL" else YELLOW if priority == "HIGH" else DIM
            type_color = GREEN if "complete" in ev_type else YELLOW if "request" in ev_type else CYAN

            # Event-specific detail
            detail = ""
            if ev_type == "container_move_complete":
                detail = f"  {ev.get('container_id', '')}  {ev.get('weight_tons', '')}t"
            elif ev_type == "support_request":
                detail = f"  {ev.get('capability_needed', '')}"
            elif ev_type == "equipment_status_change":
                detail = f"  {ev.get('status', '')} — {ev.get('details', '')[:40]}"
            elif ev_type == "capability_update":
                detail = f"  {ev.get('field', '')} = {ev.get('value', '')}"

            lines.append(
                f"  {DIM}#{i:>3}{RESET}  "
                f"{type_color}{ev_type:<28}{RESET}  "
                f"{source:<12}  "
                f"{priority_color}{priority:<8}{RESET}  "
                f"{agg}"
            )
            if detail:
                lines.append(f"       {detail}")
    else:
        lines.append(f"  {DIM}No events captured{RESET}")

    # Action history
    lines.append("")
    lines.append(sep)
    lines.append(f"  {BOLD}ACTION HISTORY{RESET} ({len(s.events)} cycles)")
    lines.append(sep)

    for ev in s.events:
        action_color = GREEN if ev["action"] == "complete_container_move" else YELLOW if ev["action"] == "request_support" else CYAN if ev["action"] == "wait" else WHITE
        status = f"{GREEN}✓{RESET}" if ev["success"] else f"{RED}✗{RESET}"
        lines.append(
            f"  {ev['sim_time']}  {status}  {action_color}{ev['action']:<28}{RESET}  "
            f"{_trunc(ev.get('result', ''), 40)}"
        )

    lines.append("")
    lines.append(sep)
    lines.append("")

    return "\n".join(lines)


def _render_dict(lines: list, d: Any, indent: int = 4, max_depth: int = 3, depth: int = 0):
    """Recursively render a dict/list as indented YAML-like output."""
    prefix = " " * indent
    if depth >= max_depth:
        lines.append(f"{prefix}{DIM}...{RESET}")
        return

    if isinstance(d, dict):
        for k, v in d.items():
            if isinstance(v, dict):
                lines.append(f"{prefix}{BOLD}{k}{RESET}:")
                _render_dict(lines, v, indent + 2, max_depth, depth + 1)
            elif isinstance(v, list):
                if len(v) > 5:
                    lines.append(f"{prefix}{BOLD}{k}{RESET}: [{len(v)} items]")
                    for item in v[:3]:
                        if isinstance(item, dict):
                            _render_dict(lines, item, indent + 2, max_depth, depth + 1)
                        else:
                            lines.append(f"{prefix}  - {item}")
                    lines.append(f"{prefix}  {DIM}... +{len(v) - 3} more{RESET}")
                else:
                    lines.append(f"{prefix}{BOLD}{k}{RESET}:")
                    for item in v:
                        if isinstance(item, dict):
                            lines.append(f"{prefix}  -")
                            _render_dict(lines, item, indent + 4, max_depth, depth + 1)
                        else:
                            lines.append(f"{prefix}  - {item}")
            else:
                # Color values by type
                if isinstance(v, bool):
                    vc = GREEN if v else RED
                    lines.append(f"{prefix}{BOLD}{k}{RESET}: {vc}{v}{RESET}")
                elif isinstance(v, (int, float)):
                    lines.append(f"{prefix}{BOLD}{k}{RESET}: {CYAN}{v}{RESET}")
                elif isinstance(v, str) and v in ("OPERATIONAL", "READY", "ACTIVE", "NORMAL"):
                    lines.append(f"{prefix}{BOLD}{k}{RESET}: {GREEN}{v}{RESET}")
                elif isinstance(v, str) and v in ("DEGRADED", "FAILED", "OFFLINE"):
                    lines.append(f"{prefix}{BOLD}{k}{RESET}: {RED}{v}{RESET}")
                else:
                    lines.append(f"{prefix}{BOLD}{k}{RESET}: {_trunc(str(v), 60)}")
    elif isinstance(d, list):
        for item in d:
            if isinstance(item, dict):
                lines.append(f"{prefix}-")
                _render_dict(lines, item, indent + 2, max_depth, depth + 1)
            else:
                lines.append(f"{prefix}- {item}")
