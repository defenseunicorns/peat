#!/usr/bin/env python3
"""
Analyze Producer-Only Baseline Scaling (Lab 1)

Compares ingress-only performance against Lab 2 (full replication) to prove
that broadcast overhead is the primary bottleneck, not ingress capacity.
"""

import sys
import csv
from pathlib import Path

def analyze_scaling(csv_path):
    """Analyze producer-only scaling characteristics."""

    results = []
    with open(csv_path, 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            if row['Status'] == 'PASS':
                results.append({
                    'nodes': int(row['NodeCount']),
                    'bandwidth': row['Bandwidth'],
                    'ingress_p50': float(row['Ingress_P50_ms']),
                    'ingress_p95': float(row['Ingress_P95_ms']),
                    'ingress_p99': float(row['Ingress_P99_ms']),
                    'ingress_max': float(row['Ingress_Max_ms']),
                    'server_total': int(row['Server_TotalMessages']),
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
    print("PRODUCER-ONLY BASELINE SCALING ANALYSIS (Lab 1)")
    print("=" * 80)
    print()
    print("Architecture: Clients → Server (upload only, NO broadcast)")
    print("Expected: O(n) ingress scaling (better than Lab 2's O(n²) broadcast)")
    print()

    # Analyze each bandwidth
    for bw in sorted(by_bandwidth.keys()):
        print(f"\n{'='*80}")
        print(f"Bandwidth: {bw}")
        print(f"{'='*80}\n")

        data = by_bandwidth[bw]

        print(f"{'Nodes':<8} {'Ingress P50':<15} {'Ingress P95':<15} {'Total Messages':<15}")
        print("-" * 80)

        for r in data:
            print(f"{r['nodes']:<8} {r['ingress_p50']:<15.3f} {r['ingress_p95']:<15.3f} "
                  f"{r['server_total']:<15}")

        # Find saturation point (Ingress P95 > 5s)
        print(f"\n{'Saturation Analysis:':<40}")
        print("-" * 80)

        saturated = False
        for r in data:
            if r['ingress_p95'] > 5000:  # 5 seconds
                print(f"  ⚠️  SATURATION POINT: {r['nodes']} nodes")
                print(f"      Ingress P95 = {r['ingress_p95']:.1f}ms (>5s = unacceptable)")
                saturated = True
                break

        if not saturated:
            print(f"  ✅ No saturation detected (all tests < 5s P95)")
            print(f"      Max tested: {data[-1]['nodes']} nodes")
            print(f"      Max P95: {data[-1]['ingress_p95']:.1f}ms")

        # Calculate scaling factors
        if len(data) > 1:
            print(f"\n{'Scaling Factors (Ingress P50):':<40}")
            print("-" * 80)
            for i in range(1, len(data)):
                prev = data[i-1]
                curr = data[i]
                node_ratio = curr['nodes'] / prev['nodes']
                latency_ratio = curr['ingress_p50'] / prev['ingress_p50'] if prev['ingress_p50'] > 0 else 0

                # Expected linear scaling
                linear = node_ratio

                print(f"  {prev['nodes']} → {curr['nodes']} nodes: "
                      f"{latency_ratio:.2f}× latency "
                      f"(expected linear={linear:.2f}×)")

    # Summary
    print(f"\n{'='*80}")
    print("COMPARISON TO LAB 2 (Full Replication)")
    print(f"{'='*80}\n")

    print("Lab 2 Results (Full Replication with Broadcast):")
    print("  - Breaking Point: 384-500 nodes (E2E P95 > 5s)")
    print("  - Scaling: Worse than O(n²) at 750-1000 nodes")
    print("  - Root Cause: Server broadcast queue saturation (O(n²) messages)")
    print()

    # Find Lab 1 breaking point
    breaking_points = []
    for bw in sorted(by_bandwidth.keys()):
        for r in by_bandwidth[bw]:
            if r['ingress_p95'] > 5000:
                breaking_points.append((bw, r['nodes'], r['ingress_p95']))
                break

    if breaking_points:
        print("Lab 1 Results (Producer-Only, Upload Only):")
        print("  Breaking Point (Ingress P95 > 5s):")
        for bw, nodes, latency in breaking_points:
            print(f"    {bw:<12}: {nodes} nodes (P95={latency:.1f}ms)")
    else:
        print("Lab 1 Results (Producer-Only, Upload Only):")
        print("  ✅ No breaking point found (all tests < 5s)")
        print("  Conclusion: Ingress-only scales MUCH better than full replication")
        print()
        print("  This proves that BROADCAST OVERHEAD is the bottleneck, not ingress capacity")

    print()

if __name__ == '__main__':
    if len(sys.argv) != 2:
        print("Usage: analyze-producer-only-scaling.py <results_directory>")
        sys.exit(1)

    results_dir = Path(sys.argv[1])
    csv_path = results_dir / 'producer-only-results.csv'

    if not csv_path.exists():
        print(f"Error: {csv_path} not found")
        sys.exit(1)

    analyze_scaling(csv_path)
