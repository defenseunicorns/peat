#!/usr/bin/env python3
"""
Compare Lab 3 (P2P Raw TCP) vs Lab 3b (P2P with HIVE CRDT)

Measures pure CRDT overhead by comparing identical topologies.
"""

import sys
import json
import re
from pathlib import Path
from collections import defaultdict
from statistics import mean, median


def parse_lab3_analysis(analysis_file):
    """Parse Lab 3 analysis.txt file."""
    results = defaultdict(dict)

    with open(analysis_file, 'r') as f:
        content = f.read()

    # Extract bandwidth sections
    bandwidth_sections = re.split(r'Bandwidth: (\w+)', content)

    for i in range(1, len(bandwidth_sections), 2):
        bandwidth = bandwidth_sections[i]
        section = bandwidth_sections[i+1]

        # Extract node data
        lines = section.split('\n')
        for line in lines:
            # Match lines like: "5        10           0.113        0.128        44"
            match = re.match(r'\s*(\d+)\s+(\d+)\s+([\d.]+)\s+([\d.]+)\s+(\d+)', line)
            if match:
                nodes = int(match.group(1))
                connections = int(match.group(2))
                p50 = float(match.group(3))
                p95 = float(match.group(4))
                total_recv = int(match.group(5))

                results[bandwidth][nodes] = {
                    'connections': connections,
                    'p50_ms': p50,
                    'p95_ms': p95,
                    'total_received': total_recv,
                }

    return results


def parse_lab3b_csv(csv_file):
    """Parse Lab 3b CSV results."""
    results = defaultdict(dict)

    with open(csv_file, 'r') as f:
        header = f.readline()  # Skip header
        for line in f:
            parts = line.strip().split(',')
            if len(parts) < 8:
                continue

            nodes = int(parts[0])
            bandwidth = parts[1]
            connections = int(parts[2])
            crdt_p50 = float(parts[3]) if parts[3] else 0
            crdt_p95 = float(parts[4]) if parts[4] else 0
            crdt_p99 = float(parts[5]) if parts[5] else 0
            crdt_max = float(parts[6]) if parts[6] else 0
            total_updates = int(parts[7]) if parts[7] else 0
            status = parts[8] if len(parts) > 8 else "UNKNOWN"

            results[bandwidth][nodes] = {
                'connections': connections,
                'crdt_p50_ms': crdt_p50,
                'crdt_p95_ms': crdt_p95,
                'crdt_p99_ms': crdt_p99,
                'crdt_max_ms': crdt_max,
                'total_updates': total_updates,
                'status': status,
            }

    return results


def compare_results(lab3_results, lab3b_results):
    """Compare Lab 3 and Lab 3b results."""

    print("=" * 100)
    print("COMPARISON: Lab 3 (Raw TCP) vs Lab 3b (HIVE CRDT)")
    print("=" * 100)
    print()

    print("Measuring CRDT Overhead: Lab 3b latency - Lab 3 latency")
    print()

    # Find common bandwidths
    common_bandwidths = set(lab3_results.keys()) & set(lab3b_results.keys())

    if not common_bandwidths:
        print("❌ No common bandwidths found between Lab 3 and Lab 3b")
        return

    for bandwidth in sorted(common_bandwidths):
        print("=" * 100)
        print(f"Bandwidth: {bandwidth}")
        print("=" * 100)
        print()

        lab3_bw = lab3_results[bandwidth]
        lab3b_bw = lab3b_results[bandwidth]

        # Find common node counts
        common_nodes = set(lab3_bw.keys()) & set(lab3b_bw.keys())

        if not common_nodes:
            print(f"  No common node counts for {bandwidth}")
            print()
            continue

        # Header
        print(f"{'Nodes':>6} {'Conn':>6} │ {'Lab 3 P95':>10} │ {'Lab 3b P95':>11} │ {'CRDT Overhead':>15} │ {'% Increase':>12}")
        print("─" * 100)

        for nodes in sorted(common_nodes):
            lab3_data = lab3_bw[nodes]
            lab3b_data = lab3b_bw[nodes]

            lab3_p95 = lab3_data['p95_ms']
            lab3b_p95 = lab3b_data.get('crdt_p95_ms', 0)

            if lab3b_p95 > 0 and lab3_p95 > 0:
                overhead = lab3b_p95 - lab3_p95
                pct_increase = (overhead / lab3_p95) * 100
                status = "✅" if abs(pct_increase) < 50 else "⚠️"
            else:
                overhead = 0
                pct_increase = 0
                status = "❓"

            print(f"{nodes:6d} {lab3_data['connections']:6d} │ "
                  f"{lab3_p95:9.3f}ms │ "
                  f"{lab3b_p95:10.3f}ms │ "
                  f"{overhead:+14.3f}ms │ "
                  f"{pct_increase:+11.1f}% {status}")

        print()

    print()
    print("INTERPRETATION")
    print("─" * 100)
    print("Positive overhead: CRDT adds latency (expected)")
    print("Negative overhead: CRDT improves latency (surprising, may indicate different workloads)")
    print("< 50% increase: Acceptable CRDT overhead ✅")
    print("> 50% increase: Significant CRDT overhead ⚠️")
    print()


def main():
    if len(sys.argv) < 3:
        print("Usage: python3 compare-lab3-vs-lab3b.py <lab3-analysis.txt> <lab3b-results-dir>")
        print()
        print("Example:")
        print("  python3 compare-lab3-vs-lab3b.py \\")
        print("    p2p-mesh-comprehensive-20251122-202726/analysis.txt \\")
        print("    hive-flat-mesh-20251123-140315")
        sys.exit(1)

    lab3_analysis = Path(sys.argv[1])
    lab3b_dir = Path(sys.argv[2])

    if not lab3_analysis.exists():
        print(f"❌ Lab 3 analysis file not found: {lab3_analysis}")
        sys.exit(1)

    lab3b_csv = lab3b_dir / "hive-flat-mesh-results.csv"
    if not lab3b_csv.exists():
        print(f"❌ Lab 3b results CSV not found: {lab3b_csv}")
        sys.exit(1)

    print("Loading Lab 3 results...")
    lab3_results = parse_lab3_analysis(lab3_analysis)

    print("Loading Lab 3b results...")
    lab3b_results = parse_lab3b_csv(lab3b_csv)

    print()
    compare_results(lab3_results, lab3b_results)

    print()
    print("Summary saved to:")
    print(f"  Lab 3:  {lab3_analysis}")
    print(f"  Lab 3b: {lab3b_csv}")


if __name__ == "__main__":
    main()
