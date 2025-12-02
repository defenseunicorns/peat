#!/usr/bin/env python3
"""
E13 Delta Sync - Bandwidth Analysis

Extracts bandwidth metrics from E13v2 and E13v3 test logs.
Focuses on message_size_bytes to calculate total bandwidth usage.
"""

import json
import re
import sys
from pathlib import Path
from collections import defaultdict
from typing import Dict, List
import statistics


def extract_metrics_from_logs(test_dir: Path) -> List[Dict]:
    """Extract all METRICS lines from log files"""
    metrics = []
    log_files = list(test_dir.glob("*.log"))

    for log_file in log_files:
        with open(log_file) as f:
            for line in f:
                if "METRICS:" in line:
                    match = re.search(r'METRICS:\s*({.*})', line)
                    if match:
                        try:
                            metric = json.loads(match.group(1))
                            metrics.append(metric)
                        except json.JSONDecodeError:
                            continue
    return metrics


def analyze_bandwidth(test_dir: Path, test_name: str) -> Dict:
    """Analyze bandwidth usage from metrics"""
    metrics = extract_metrics_from_logs(test_dir)

    if not metrics:
        return {
            "test_name": test_name,
            "error": "No metrics found"
        }

    # Count messages sent
    messages_sent = [m for m in metrics if m.get("event_type") == "MessageSent"]
    docs_received = [m for m in metrics if m.get("event_type") == "DocumentReceived"]
    docs_inserted = [m for m in metrics if m.get("event_type") == "DocumentInserted"]

    # Calculate total bytes sent
    total_bytes = sum(m.get("message_size_bytes", 0) for m in messages_sent)

    # Get latencies if available
    latencies = [m["latency_ms"] for m in docs_received if "latency_ms" in m]

    # Count nodes
    nodes = set()
    for m in metrics:
        if "node_id" in m:
            nodes.add(m["node_id"])

    result = {
        "test_name": test_name,
        "node_count": len(nodes),
        "messages_sent": len(messages_sent),
        "docs_received": len(docs_received),
        "docs_inserted": len(docs_inserted),
        "total_bytes_sent": total_bytes,
        "avg_message_size": total_bytes / len(messages_sent) if messages_sent else 0,
    }

    # Add latency stats if available
    if latencies:
        # Filter outliers (> 60 seconds suggests test timing issues)
        valid_latencies = [l for l in latencies if l < 60000]
        if valid_latencies:
            result["latency"] = {
                "sample_count": len(valid_latencies),
                "min_ms": min(valid_latencies),
                "max_ms": max(valid_latencies),
                "avg_ms": statistics.mean(valid_latencies),
                "median_ms": statistics.median(valid_latencies),
            }

    return result


def main():
    scripts_dir = Path(__file__).parent

    e13v2_dir = scripts_dir / "e13v2-full-matrix-20251114-114826"
    e13v3_dir = scripts_dir / "e13v3-mode4-hierarchical-20251114-121905"

    print("=" * 80)
    print("E13 Delta Sync Validation - Bandwidth Analysis")
    print("=" * 80)
    print()

    # Analyze E13v2 (P2P Mesh)
    print("## E13v2: P2P Mesh (Limited Connections)")
    print()
    e13v2_results = []
    for test_dir in sorted(e13v2_dir.iterdir()):
        if test_dir.is_dir():
            result = analyze_bandwidth(test_dir, test_dir.name)
            e13v2_results.append(result)

            if "error" in result:
                print(f"  {result['test_name']}: {result['error']}")
                continue

            print(f"### {result['test_name']}")
            print(f"  - Nodes: {result['node_count']}")
            print(f"  - Messages Sent: {result['messages_sent']}")
            print(f"  - Total Bytes: {result['total_bytes_sent']:,} ({result['total_bytes_sent']/1024:.1f} KB)")
            print(f"  - Avg Message Size: {result['avg_message_size']:.0f} bytes")
            if "latency" in result:
                lat = result["latency"]
                print(f"  - Latency: {lat['min_ms']:.1f} - {lat['max_ms']:.1f}ms (median: {lat['median_ms']:.1f}ms)")
            print()

    # Analyze E13v3 (Mode 4)
    print("## E13v3: Mode 4 Hierarchical Aggregation")
    print()
    e13v3_results = []
    for test_dir in sorted(e13v3_dir.iterdir()):
        if test_dir.is_dir():
            result = analyze_bandwidth(test_dir, test_dir.name)
            e13v3_results.append(result)

            if "error" in result:
                print(f"  {result['test_name']}: {result['error']}")
                continue

            print(f"### {result['test_name']}")
            print(f"  - Nodes: {result['node_count']}")
            print(f"  - Messages Sent: {result['messages_sent']}")
            print(f"  - Total Bytes: {result['total_bytes_sent']:,} ({result['total_bytes_sent']/1024:.1f} KB)")
            print(f"  - Avg Message Size: {result['avg_message_size']:.0f} bytes")
            if "latency" in result:
                lat = result["latency"]
                print(f"  - Latency: {lat['min_ms']:.1f} - {lat['max_ms']:.1f}ms (median: {lat['median_ms']:.1f}ms)")
            print()

    # Comparison table
    print("=" * 80)
    print("## Bandwidth Comparison: P2P Mesh vs Mode 4 Hierarchical")
    print("=" * 80)
    print()
    print("| Scale | Architecture | Nodes | Messages | Total KB | Avg Msg (B) | Bandwidth Reduction |")
    print("|-------|--------------|-------|----------|----------|-------------|---------------------|")

    # Match tests by scale
    for v2_test in e13v2_results:
        if "error" in v2_test:
            continue

        # Parse scale
        match = re.search(r'(\d+)node', v2_test['test_name'])
        if not match:
            continue
        scale = match.group(1)

        # Find matching E13v3 test
        v3_test = None
        for test in e13v3_results:
            if f"{scale}node" in test['test_name'] and "error" not in test:
                v3_test = test
                break

        # Print P2P row
        v2_kb = v2_test['total_bytes_sent'] / 1024
        print(f"| {scale}node | P2P Mesh | {v2_test['node_count']} | {v2_test['messages_sent']} | "
              f"{v2_kb:.1f} | {v2_test['avg_message_size']:.0f} | - |")

        # Print Mode 4 row and calculate reduction
        if v3_test:
            v3_kb = v3_test['total_bytes_sent'] / 1024
            reduction = 0
            if v2_test['total_bytes_sent'] > 0:
                reduction = (1 - v3_test['total_bytes_sent'] / v2_test['total_bytes_sent']) * 100

            reduction_str = f"{reduction:.1f}%" if reduction > 0 else f"{abs(reduction):.1f}% increase"

            print(f"| {scale}node | Mode 4 Hier | {v3_test['node_count']} | {v3_test['messages_sent']} | "
                  f"{v3_kb:.1f} | {v3_test['avg_message_size']:.0f} | {reduction_str} |")

    print()

    # Save results
    output_file = scripts_dir / "e13-bandwidth-analysis.json"
    with open(output_file, "w") as f:
        json.dump({
            "e13v2": e13v2_results,
            "e13v3": e13v3_results
        }, f, indent=2)

    print(f"✓ Results saved to {output_file}")


if __name__ == "__main__":
    main()
