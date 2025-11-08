#!/usr/bin/env python3
"""
Three-Way Architectural Comparison Analysis
Compares Traditional IoT vs CAP Full vs CAP Differential
"""

import json
import sys
from collections import defaultdict
from pathlib import Path

def extract_metrics(log_file):
    """Extract JSON metrics from log file"""
    metrics = []
    with open(log_file) as f:
        for line in f:
            if "METRICS:" in line:
                try:
                    json_str = line.split("METRICS:")[1].strip()
                    metric = json.loads(json_str)
                    metrics.append(metric)
                except Exception as e:
                    continue
    return metrics

def analyze_bandwidth(metrics):
    """Calculate bandwidth statistics from metrics"""
    sent_msgs = [m for m in metrics if m.get("event_type") == "MessageSent"]

    if not sent_msgs:
        return {}

    total_bytes = sum(m.get("message_size_bytes", 0) for m in sent_msgs)
    avg_msg_size = total_bytes / len(sent_msgs) if sent_msgs else 0

    # Group by node to calculate per-node stats
    by_node = defaultdict(list)
    for m in sent_msgs:
        by_node[m.get("node_id", "unknown")].append(m)

    return {
        "total_messages": len(sent_msgs),
        "total_bytes": total_bytes,
        "avg_message_size": avg_msg_size,
        "unique_nodes": len(by_node),
        "messages_per_node": {node: len(msgs) for node, msgs in by_node.items()},
        "bytes_per_node": {node: sum(m.get("message_size_bytes", 0) for m in msgs)
                          for node, msgs in by_node.items()}
    }

def analyze_latency(metrics):
    """Calculate latency statistics"""
    latency_metrics = [m for m in metrics if "latency_us" in m]

    if not latency_metrics:
        return {}

    latencies = [m["latency_us"] / 1000 for m in latency_metrics]  # Convert to ms

    return {
        "count": len(latencies),
        "avg_latency_ms": sum(latencies) / len(latencies),
        "min_latency_ms": min(latencies),
        "max_latency_ms": max(latencies),
    }

def generate_comparison_report(traditional_file, cap_full_file=None, cap_differential_file=None):
    """Generate three-way comparison report"""

    print("=" * 80)
    print("THREE-WAY ARCHITECTURAL COMPARISON")
    print("=" * 80)
    print()

    # Extract metrics from all sources
    traditional_metrics = extract_metrics(traditional_file)
    print(f"✓ Loaded {len(traditional_metrics)} metrics from Traditional IoT Baseline")

    # Analyze Traditional IoT
    trad_bw = analyze_bandwidth(traditional_metrics)
    trad_lat = analyze_latency(traditional_metrics)

    print()
    print("─" * 80)
    print("1. TRADITIONAL IoT BASELINE (NO CRDT)")
    print("─" * 80)
    print(f"Architecture: Event-driven periodic full-state messaging")
    print(f"Data Model:   Full messages (no deltas, no CRDT)")
    print(f"Sync:         Last-write-wins")
    print()
    print("Bandwidth Metrics:")
    print(f"  • Total messages sent: {trad_bw['total_messages']}")
    print(f"  • Total bytes transmitted: {trad_bw['total_bytes']:,} bytes")
    print(f"  • Average message size: {trad_bw['avg_message_size']:.1f} bytes")
    print(f"  • Active nodes: {trad_bw['unique_nodes']}")
    if trad_lat:
        print()
        print("Latency Metrics:")
        print(f"  • Measurements: {trad_lat['count']}")
        print(f"  • Average latency: {trad_lat['avg_latency_ms']:.2f} ms")
        print(f"  • Min/Max latency: {trad_lat['min_latency_ms']:.2f} / {trad_lat['max_latency_ms']:.2f} ms")

    # Note about comparison
    print()
    print("─" * 80)
    print("COMPARISON NOTES")
    print("─" * 80)
    print()
    print("Traditional IoT Baseline demonstrates:")
    print("  ✓ Simple implementation (no CRDT overhead)")
    print("  ✓ Periodic full-state transmission")
    print("  ✓ No automatic convergence")
    print("  ✓ No capability-based filtering")
    print()
    print("Expected results when compared to CAP Protocol:")
    print()
    print("  1. CRDT Overhead (Traditional → CAP Full):")
    print("     • CAP Full should show ~20-40% bandwidth reduction via delta-state CRDTs")
    print("     • Automatic conflict resolution and eventual consistency")
    print("     • More complex implementation")
    print()
    print("  2. CAP Filtering Benefit (CAP Full → CAP Differential):")
    print("     • CAP Differential should show additional ~30-50% reduction")
    print("     • Role-based authorization reduces unnecessary data transmission")
    print("     • Same CRDT benefits plus capability filtering")
    print()
    print("  3. Net Architectural Advantage (Traditional → CAP Differential):")
    print("     • Combined benefit: ~50-70% bandwidth reduction")
    print("     • Automatic convergence + security + efficiency")
    print()

    if cap_full_file:
        cap_full_metrics = extract_metrics(cap_full_file)
        cap_full_bw = analyze_bandwidth(cap_full_metrics)

        print("─" * 80)
        print("2. CAP FULL REPLICATION (CRDT without filtering)")
        print("─" * 80)
        print(f"  • Messages: {cap_full_bw['total_messages']}")
        print(f"  • Bytes: {cap_full_bw['total_bytes']:,}")
        print(f"  • Avg size: {cap_full_bw['avg_message_size']:.1f} bytes")
        print()

        # Calculate CRDT overhead
        if trad_bw['total_bytes'] > 0:
            crdt_reduction = (1 - cap_full_bw['total_bytes'] / trad_bw['total_bytes']) * 100
            print(f"CRDT Benefit vs Traditional: {crdt_reduction:+.1f}% bandwidth change")
            if crdt_reduction > 0:
                print(f"  (CRDT delta-state sync REDUCES bandwidth by {crdt_reduction:.1f}%)")
            else:
                print(f"  (CRDT adds {abs(crdt_reduction):.1f}% overhead)")
        print()

    if cap_differential_file:
        cap_diff_metrics = extract_metrics(cap_differential_file)
        cap_diff_bw = analyze_bandwidth(cap_diff_metrics)

        print("─" * 80)
        print("3. CAP DIFFERENTIAL FILTERING (CRDT + Capability filtering)")
        print("─" * 80)
        print(f"  • Messages: {cap_diff_bw['total_messages']}")
        print(f"  • Bytes: {cap_diff_bw['total_bytes']:,}")
        print(f"  • Avg size: {cap_diff_bw['avg_message_size']:.1f} bytes")
        print()

        # Calculate filtering benefit
        if cap_full_file and cap_full_bw['total_bytes'] > 0:
            filter_reduction = (1 - cap_diff_bw['total_bytes'] / cap_full_bw['total_bytes']) * 100
            print(f"CAP Filtering Benefit: {filter_reduction:.1f}% bandwidth reduction vs CAP Full")

        # Calculate net advantage
        if trad_bw['total_bytes'] > 0:
            net_reduction = (1 - cap_diff_bw['total_bytes'] / trad_bw['total_bytes']) * 100
            print(f"Net Advantage: {net_reduction:.1f}% bandwidth reduction vs Traditional")
        print()

    print("=" * 80)

def main():
    # Accept command line arguments or use defaults
    if len(sys.argv) >= 2:
        traditional_file = sys.argv[1]
    else:
        traditional_file = "test-traditional-baseline-run.log"

    if len(sys.argv) >= 3:
        cap_full_file = sys.argv[2]
    else:
        cap_full_file = None

    if len(sys.argv) >= 4:
        cap_differential_file = sys.argv[3]
    else:
        cap_differential_file = None

    if not Path(traditional_file).exists():
        print(f"ERROR: Traditional baseline log not found: {traditional_file}")
        sys.exit(1)

    # Check for E8 results if not provided
    if not cap_full_file or not cap_differential_file:
        e8_results = list(Path(".").glob("e8-performance-results-*/"))
        if e8_results:
            print(f"\nNote: Found E8 Phase 1 results directory: {e8_results[0]}")
            print("      Add CAP Full and CAP Differential log files for complete comparison\n")

    generate_comparison_report(traditional_file, cap_full_file, cap_differential_file)

if __name__ == "__main__":
    main()
