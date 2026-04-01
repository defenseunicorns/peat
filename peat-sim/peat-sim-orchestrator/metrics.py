#!/usr/bin/env python3
"""
peat-sim metrics aggregator.

Reads JSONL metrics lines (prefixed with "METRICS:") from per-node log files
produced by the orchestrator and computes per-tier latency distributions.

Outputs a CSV file and prints a human-readable summary table.
"""

import argparse
import csv
import json
import math
import os
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional


# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------

@dataclass
class LatencySample:
    """A single latency measurement."""
    latency_ms: float
    source_tier: str
    dest_tier: str
    node_id: str
    is_warmup: bool = False


@dataclass
class AggregationEvent:
    """An aggregation completed event."""
    node_id: str
    tier: str
    input_doc_type: str
    output_doc_type: str
    input_count: int
    processing_time_us: int
    input_bytes: Optional[int] = None
    output_bytes: Optional[int] = None


@dataclass
class TierStats:
    """Aggregated statistics for a tier."""
    tier_label: str
    latencies_ms: List[float] = field(default_factory=list)
    aggregation_count: int = 0
    total_processing_time_us: int = 0


# ---------------------------------------------------------------------------
# Parsing
# ---------------------------------------------------------------------------

def parse_metrics_line(line: str) -> Optional[dict]:
    """Extract a JSON dict from a METRICS: prefixed line."""
    idx = line.find("METRICS:")
    if idx < 0:
        return None
    json_str = line[idx + len("METRICS:"):].strip()
    if not json_str:
        return None
    try:
        return json.loads(json_str)
    except json.JSONDecodeError:
        return None


def classify_latency_tier(event: dict) -> Optional[str]:
    """
    Map a DocumentReceived event to a tier label based on latency_type or
    source_tier/dest_tier fields.

    Returns a canonical tier label like:
      "soldier -> squad_leader"
      "squad_leader -> platoon_leader"
      "platoon_leader -> company_commander"
    or None if unclassifiable.
    """
    lt = event.get("latency_type", "")
    src = event.get("source_tier", "")
    dst = event.get("dest_tier", "")

    # Try latency_type first
    if "soldier_to_squad" in lt or ("soldier" in lt and "squad" in lt):
        return "soldier -> squad_leader"
    if "squad_to_platoon" in lt or ("squad" in lt and "platoon" in lt):
        return "squad_leader -> platoon_leader"
    if "platoon_to_company" in lt or ("platoon" in lt and "company" in lt):
        return "platoon_leader -> company_commander"
    if "company_to_battalion" in lt or ("company" in lt and "battalion" in lt):
        return "company_commander -> battalion"

    # Fall back to source_tier -> dest_tier
    if src and dst:
        return f"{src} -> {dst}"

    return None


# ---------------------------------------------------------------------------
# Percentile computation (stdlib only)
# ---------------------------------------------------------------------------

def percentile(sorted_values: List[float], pct: float) -> float:
    """Compute the pct-th percentile from an already-sorted list."""
    if not sorted_values:
        return 0.0
    n = len(sorted_values)
    if n == 1:
        return sorted_values[0]
    k = (pct / 100.0) * (n - 1)
    lo = int(math.floor(k))
    hi = min(lo + 1, n - 1)
    frac = k - lo
    return sorted_values[lo] + frac * (sorted_values[hi] - sorted_values[lo])


# ---------------------------------------------------------------------------
# Main logic
# ---------------------------------------------------------------------------

def collect_metrics(log_dir: Path) -> tuple:
    """
    Scan all .log files in log_dir for METRICS: lines.

    Returns:
        (tier_stats_dict, aggregation_events_list)
    """
    tier_stats: Dict[str, TierStats] = {}
    aggregation_events: List[AggregationEvent] = []
    files_scanned = 0
    lines_parsed = 0
    metrics_found = 0

    log_files = sorted(log_dir.glob("*.log"))
    if not log_files:
        print(f"WARNING: No .log files found in {log_dir}", file=sys.stderr)
        return tier_stats, aggregation_events

    for log_file in log_files:
        files_scanned += 1
        try:
            with open(log_file, "r", errors="replace") as fh:
                for line in fh:
                    lines_parsed += 1
                    event = parse_metrics_line(line)
                    if event is None:
                        continue
                    metrics_found += 1
                    event_type = event.get("event_type", "")

                    # DocumentReceived -> latency samples
                    if event_type == "DocumentReceived":
                        is_warmup = event.get("is_warmup", False)
                        if is_warmup:
                            continue

                        tier_label = classify_latency_tier(event)
                        if tier_label is None:
                            continue

                        latency_ms = event.get("latency_ms")
                        if latency_ms is None:
                            latency_us = event.get("latency_us")
                            if latency_us is not None:
                                latency_ms = latency_us / 1000.0
                            else:
                                continue

                        if tier_label not in tier_stats:
                            tier_stats[tier_label] = TierStats(tier_label=tier_label)
                        tier_stats[tier_label].latencies_ms.append(latency_ms)

                    # AggregationCompleted -> aggregation tracking
                    elif event_type == "AggregationCompleted":
                        ae = AggregationEvent(
                            node_id=event.get("node_id", ""),
                            tier=event.get("tier", ""),
                            input_doc_type=event.get("input_doc_type", ""),
                            output_doc_type=event.get("output_doc_type", ""),
                            input_count=event.get("input_count", 0),
                            processing_time_us=event.get("processing_time_us", 0),
                            input_bytes=event.get("input_bytes"),
                            output_bytes=event.get("output_bytes"),
                        )
                        aggregation_events.append(ae)

                        tier_key = f"aggregation:{ae.tier}"
                        if tier_key not in tier_stats:
                            tier_stats[tier_key] = TierStats(tier_label=tier_key)
                        tier_stats[tier_key].aggregation_count += 1
                        tier_stats[tier_key].total_processing_time_us += ae.processing_time_us

        except OSError as exc:
            print(f"WARNING: Could not read {log_file}: {exc}", file=sys.stderr)

    print(f"Scanned {files_scanned} files, {lines_parsed} lines, "
          f"{metrics_found} metrics events", file=sys.stderr)

    return tier_stats, aggregation_events


def compute_output_rows(tier_stats: Dict[str, TierStats]) -> List[dict]:
    """Compute per-tier p50/p95/p99 and return as list of row dicts."""
    rows: List[dict] = []

    # Latency tiers in logical order
    latency_order = [
        "soldier -> squad_leader",
        "squad_leader -> platoon_leader",
        "platoon_leader -> company_commander",
        "company_commander -> battalion",
    ]

    # First output known tiers in order, then any extras
    seen = set()
    for tier_label in latency_order:
        if tier_label in tier_stats:
            seen.add(tier_label)
            ts = tier_stats[tier_label]
            if ts.latencies_ms:
                sorted_lat = sorted(ts.latencies_ms)
                rows.append({
                    "tier": tier_label,
                    "p50_ms": round(percentile(sorted_lat, 50), 3),
                    "p95_ms": round(percentile(sorted_lat, 95), 3),
                    "p99_ms": round(percentile(sorted_lat, 99), 3),
                    "sample_count": len(sorted_lat),
                })

    # Any additional latency tiers not in the standard order
    for tier_label, ts in sorted(tier_stats.items()):
        if tier_label in seen or tier_label.startswith("aggregation:"):
            continue
        seen.add(tier_label)
        if ts.latencies_ms:
            sorted_lat = sorted(ts.latencies_ms)
            rows.append({
                "tier": tier_label,
                "p50_ms": round(percentile(sorted_lat, 50), 3),
                "p95_ms": round(percentile(sorted_lat, 95), 3),
                "p99_ms": round(percentile(sorted_lat, 99), 3),
                "sample_count": len(sorted_lat),
            })

    return rows


def print_summary(rows: List[dict], aggregation_events: List[AggregationEvent]) -> None:
    """Print a human-readable summary table to stdout."""
    print()
    print("=" * 72)
    print("  PEAT-SIM METRICS SUMMARY")
    print("=" * 72)
    print()

    if rows:
        # Latency table
        header = f"  {'Tier':<40s} {'p50':>8s} {'p95':>8s} {'p99':>8s} {'Count':>8s}"
        print(header)
        print("  " + "-" * 68)
        for r in rows:
            print(f"  {r['tier']:<40s} {r['p50_ms']:>7.1f}ms {r['p95_ms']:>7.1f}ms "
                  f"{r['p99_ms']:>7.1f}ms {r['sample_count']:>8d}")
        print()
    else:
        print("  No latency samples found.")
        print()

    # Aggregation summary
    if aggregation_events:
        agg_by_tier: Dict[str, List[AggregationEvent]] = {}
        for ae in aggregation_events:
            agg_by_tier.setdefault(ae.tier, []).append(ae)

        print("  Aggregation Events:")
        print("  " + "-" * 68)
        print(f"  {'Tier':<20s} {'Count':>8s} {'Avg Processing':>16s} {'Avg Inputs':>12s}")
        print("  " + "-" * 68)
        for tier in sorted(agg_by_tier.keys()):
            events = agg_by_tier[tier]
            count = len(events)
            avg_proc_us = sum(e.processing_time_us for e in events) / count if count else 0
            avg_inputs = sum(e.input_count for e in events) / count if count else 0
            print(f"  {tier:<20s} {count:>8d} {avg_proc_us / 1000:>13.1f}ms {avg_inputs:>12.1f}")

        # Byte savings
        events_with_bytes = [e for e in aggregation_events
                             if e.input_bytes is not None and e.output_bytes is not None]
        if events_with_bytes:
            total_in = sum(e.input_bytes for e in events_with_bytes)
            total_out = sum(e.output_bytes for e in events_with_bytes)
            saved_pct = ((total_in - total_out) / total_in * 100) if total_in > 0 else 0
            print()
            print(f"  Bandwidth savings: {total_in:,} bytes in -> {total_out:,} bytes out "
                  f"({saved_pct:.1f}% reduction)")

        print()
    else:
        print("  No aggregation events found.")
        print()

    print("=" * 72)


def write_csv(rows: List[dict], output_path: Path) -> None:
    """Write the latency results to a CSV file."""
    fieldnames = ["tier", "p50_ms", "p95_ms", "p99_ms", "sample_count"]
    with open(output_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)
    print(f"CSV written to: {output_path}", file=sys.stderr)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def parse_args(argv: Optional[List[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Aggregate peat-sim metrics from log files",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--log-dir", type=str, required=True,
                        help="Directory containing per-node .log files")
    parser.add_argument("--output", type=str, default="metrics.csv",
                        help="Output CSV file path")
    return parser.parse_args(argv)


def main(argv: Optional[List[str]] = None) -> None:
    args = parse_args(argv)

    log_dir = Path(args.log_dir).resolve()
    if not log_dir.is_dir():
        print(f"ERROR: Log directory does not exist: {log_dir}", file=sys.stderr)
        sys.exit(1)

    output_path = Path(args.output).resolve()

    # Collect and analyze
    tier_stats, aggregation_events = collect_metrics(log_dir)
    rows = compute_output_rows(tier_stats)

    # Output
    print_summary(rows, aggregation_events)
    write_csv(rows, output_path)


if __name__ == "__main__":
    main()
