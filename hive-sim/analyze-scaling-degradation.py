#!/usr/bin/env python3
"""
Analyze Traditional Baseline Scaling Degradation

Processes traditional-results.csv to identify:
1. Where E2E latency starts degrading (breaking point)
2. Scaling curve (linear vs quadratic)
3. Comparison of broadcast vs E2E efficiency
"""

import sys
import csv
from pathlib import Path

def analyze_scaling(csv_path):
    """Analyze scaling characteristics from test results."""

    results = []
    with open(csv_path, 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            if row['Status'] == 'PASS':
                results.append({
                    'nodes': int(row['NodeCount']),
                    'bandwidth': row['Bandwidth'],
                    'broadcast_p50': float(row['Broadcast_P50_ms']),
                    'broadcast_p95': float(row['Broadcast_P95_ms']),
                    'e2e_p50': float(row['E2E_P50_ms']),
                    'e2e_p95': float(row['E2E_P95_ms']),
                    'e2e_max': float(row['E2E_Max_ms'])
                })

    # Group by bandwidth
    by_bandwidth = {}
    for r in results:
        bw = r['bandwidth']
        if bw not in by_bandwidth:
            by_bandwidth[bw] = []
        by_bandwidth[bw].append(r)

    # Sort by node count
    for bw in by_bandwidth:
        by_bandwidth[bw].sort(key=lambda x: x['nodes'])

    print("=" * 80)
    print("TRADITIONAL BASELINE SCALING ANALYSIS")
    print("=" * 80)
    print()

    # Analyze each bandwidth
    for bw in sorted(by_bandwidth.keys()):
        print(f"\n{'='*80}")
        print(f"Bandwidth: {bw}")
        print(f"{'='*80}\n")

        data = by_bandwidth[bw]

        print(f"{'Nodes':<8} {'Broadcast P50':<15} {'E2E P50':<15} {'E2E/BC Ratio':<15} {'E2E P95':<15}")
        print("-" * 80)

        for r in data:
            ratio = r['e2e_p50'] / r['broadcast_p50'] if r['broadcast_p50'] > 0 else 0
            print(f"{r['nodes']:<8} {r['broadcast_p50']:<15.3f} {r['e2e_p50']:<15.3f} "
                  f"{ratio:<15.1f}× {r['e2e_p95']:<15.3f}")

        # Calculate degradation points
        print(f"\n{'Degradation Analysis:':<40}")
        print("-" * 80)

        # Find where E2E P50 exceeds thresholds
        thresholds = [500, 1000, 5000, 10000]  # ms
        for threshold in thresholds:
            for i, r in enumerate(data):
                if r['e2e_p50'] > threshold:
                    print(f"  E2E P50 > {threshold}ms at: {r['nodes']} nodes ({r['e2e_p50']:.1f}ms)")
                    break

        # Find where E2E P95 becomes unacceptable (>5s)
        for i, r in enumerate(data):
            if r['e2e_p95'] > 5000:
                print(f"\n  ⚠️  BREAKING POINT: {r['nodes']} nodes")
                print(f"      E2E P95 = {r['e2e_p95']:.1f}ms (>5s = unacceptable)")
                break

        # Calculate scaling factor between node counts
        if len(data) > 1:
            print(f"\n{'Scaling Factors (E2E P50):':<40}")
            print("-" * 80)
            for i in range(1, len(data)):
                prev = data[i-1]
                curr = data[i]
                node_ratio = curr['nodes'] / prev['nodes']
                latency_ratio = curr['e2e_p50'] / prev['e2e_p50'] if prev['e2e_p50'] > 0 else 0

                # Expected ratios
                linear = node_ratio
                quadratic = node_ratio ** 2

                print(f"  {prev['nodes']} → {curr['nodes']} nodes: "
                      f"{latency_ratio:.2f}× latency "
                      f"(linear={linear:.2f}×, quadratic={quadratic:.2f}×)")

    # Summary
    print(f"\n{'='*80}")
    print("SUMMARY")
    print(f"{'='*80}\n")

    # Find practical breaking point (E2E P95 > 5s)
    breaking_points = []
    for bw in sorted(by_bandwidth.keys()):
        for r in by_bandwidth[bw]:
            if r['e2e_p95'] > 5000:
                breaking_points.append((bw, r['nodes'], r['e2e_p95']))
                break

    if breaking_points:
        print("Breaking Point (E2E P95 > 5s):")
        for bw, nodes, latency in breaking_points:
            print(f"  {bw:<12}: {nodes} nodes (P95={latency:.1f}ms)")
    else:
        print("No breaking point found (all tests < 5s P95)")

    print()

if __name__ == '__main__':
    if len(sys.argv) != 2:
        print("Usage: analyze-scaling-degradation.py <results_directory>")
        sys.exit(1)

    results_dir = Path(sys.argv[1])
    csv_path = results_dir / 'traditional-results.csv'

    if not csv_path.exists():
        print(f"Error: {csv_path} not found")
        sys.exit(1)

    analyze_scaling(csv_path)
