#!/usr/bin/env python3
"""
Analyze E12 comprehensive validation results
Compares Traditional vs CAP Full vs CAP Hierarchical scaling
"""

import json
import sys
from pathlib import Path
import math

def analyze_results(results_dir):
    results_dir = Path(results_dir)

    if not results_dir.exists():
        print(f"Error: Results directory not found: {results_dir}")
        sys.exit(1)

    # Collect all test results
    tests = {}

    for test_dir in sorted(results_dir.iterdir()):
        if not test_dir.is_dir():
            continue

        # Parse test name: architecture-scale-bandwidth
        parts = test_dir.name.split('-')
        if len(parts) < 3:
            continue

        bandwidth = parts[-1]
        scale = parts[-2]
        architecture = '-'.join(parts[:-2])

        # Load Docker stats
        docker_stats = test_dir / "docker-stats-summary.json"
        if not docker_stats.exists():
            continue

        with open(docker_stats) as f:
            stats = json.load(f)
            total_bytes = sum(node.get("net_total_bytes", 0) for node in stats.values())
            node_count = len(stats)

        # Load test metrics (including latency)
        test_summary = test_dir / "test-summary.json"
        latency = {}
        if test_summary.exists():
            with open(test_summary) as f:
                metrics = json.load(f)
                if "latency_avg_ms" in metrics:
                    latency = {
                        "avg": metrics.get("latency_avg_ms"),
                        "median": metrics.get("latency_median_ms"),
                        "p90": metrics.get("latency_p90_ms"),
                        "p99": metrics.get("latency_p99_ms"),
                        "count": metrics.get("latency_count", 0)
                    }

        key = (architecture, scale, bandwidth)
        tests[key] = {
            "total_mb": total_bytes / 1e6,
            "node_count": node_count,
            "per_node_kb": (total_bytes / 1e6 * 1024) / node_count if node_count > 0 else 0,
            "latency": latency
        }

    # Group by architecture and bandwidth
    architectures = {}
    for (arch, scale, bw), data in tests.items():
        if arch not in architectures:
            architectures[arch] = {}
        if bw not in architectures[arch]:
            architectures[arch][bw] = {}
        architectures[arch][bw][scale] = data

    # Generate report
    print("=" * 80)
    print("E12 Comprehensive Empirical Validation Results")
    print("=" * 80)
    print()

    # 1Gbps scaling comparison
    print("## Scaling Comparison @ 1Gbps")
    print()

    scales_1gbps = {}
    for arch in ["traditional", "cap-full", "cap-hierarchical"]:
        if arch in architectures and "1gbps" in architectures[arch]:
            scales_1gbps[arch] = architectures[arch]["1gbps"]

    if scales_1gbps:
        all_scales = set()
        for arch_data in scales_1gbps.values():
            all_scales.update(arch_data.keys())

        scales_sorted = sorted(all_scales, key=lambda x: int(x.replace("node", "")))

        print(f"{'Scale':<10} | {'Traditional':<15} | {'CAP Full':<15} | {'CAP Hierarchical':<15}")
        print("-" * 80)

        for scale in scales_sorted:
            row = [scale]
            for arch in ["traditional", "cap-full", "cap-hierarchical"]:
                if arch in scales_1gbps and scale in scales_1gbps[arch]:
                    data = scales_1gbps[arch][scale]
                    row.append(f"{data['total_mb']:.2f} MB")
                else:
                    row.append("N/A")
            print(f"{row[0]:<10} | {row[1]:<15} | {row[2]:<15} | {row[3]:<15}")

        print()

        # Calculate scaling complexity
        print("## Empirical Scaling Complexity")
        print()

        for arch in ["traditional", "cap-full", "cap-hierarchical"]:
            if arch not in scales_1gbps:
                continue

            arch_data = scales_1gbps[arch]
            if len(arch_data) < 2:
                continue

            points = []
            for scale, data in arch_data.items():
                nodes = int(scale.replace("node", ""))
                points.append((nodes, data['total_mb']))
            points.sort()

            if len(points) >= 2:
                first, last = points[0], points[-1]
                node_ratio = last[0] / first[0]
                traffic_ratio = last[1] / first[1]

                if node_ratio > 1 and traffic_ratio > 1:
                    complexity = math.log(traffic_ratio) / math.log(node_ratio)

                    print(f"**{arch}**: O(n^{complexity:.2f})")
                    print(f"  {first[0]} nodes: {first[1]:.2f} MB → {last[0]} nodes: {last[1]:.2f} MB")
                    print(f"  {node_ratio:.1f}x nodes → {traffic_ratio:.1f}x traffic")
                    print()

    # Latency Analysis @ 1Gbps
    print("## Latency Analysis @ 1Gbps")
    print()

    if scales_1gbps:
        print(f"{'Scale':<10} | {'Architecture':<18} | {'Avg':<10} | {'Median':<10} | {'p90':<10} | {'p99':<10} | {'Samples':<10}")
        print("-" * 100)

        for scale in scales_sorted:
            for arch in ["traditional", "cap-full", "cap-hierarchical"]:
                if arch in scales_1gbps and scale in scales_1gbps[arch]:
                    data = scales_1gbps[arch][scale]
                    if data['latency'] and data['latency'].get('avg'):
                        lat = data['latency']
                        print(f"{scale:<10} | {arch:<18} | {lat['avg']:>8.2f}ms | {lat['median']:>8.2f}ms | {lat['p90']:>8.2f}ms | {lat['p99']:>8.2f}ms | {lat['count']:>8}")

        print()

    # Bandwidth constraint effects
    print("## Bandwidth Constraint Effects (24 nodes)")
    print()

    for arch in ["traditional", "cap-full", "cap-hierarchical"]:
        if arch not in architectures:
            continue

        print(f"### {arch}")
        print(f"{'Bandwidth':<12} | {'Traffic':<12} | {'Avg Latency':<15}")
        print("-" * 45)

        for bw in ["1gbps", "100mbps", "1mbps", "256kbps"]:
            if bw in architectures[arch] and "24node" in architectures[arch][bw]:
                data = architectures[arch][bw]["24node"]
                lat_str = "N/A"
                if data['latency'] and data['latency'].get('avg'):
                    lat_str = f"{data['latency']['avg']:.2f}ms"
                print(f"{bw:<12} | {data['total_mb']:>8.2f} MB | {lat_str:<15}")
        print()

    # Summary
    print(f"Results directory: {results_dir}")
    print(f"Total tests analyzed: {len(tests)}")

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python3 analyze-comprehensive-results.py <results-directory>")
        sys.exit(1)

    analyze_results(sys.argv[1])
