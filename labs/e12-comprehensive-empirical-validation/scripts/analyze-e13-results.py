#!/usr/bin/env python3
"""
E13 Delta Sync Validation - Results Analysis

Extracts bandwidth and latency metrics from E13v2 (P2P mesh) and E13v3 (Mode 4 hierarchical) test logs.
"""

import json
import re
import sys
from pathlib import Path
from collections import defaultdict
from typing import Dict, List
import statistics


class E13TestAnalyzer:
    """Analyzes E13 test results from log files"""

    def __init__(self, test_dir: Path, test_name: str):
        self.test_dir = test_dir
        self.test_name = test_name
        self.metrics = []
        self._parse_logs()

    def _parse_logs(self):
        """Parse all log files in directory and extract METRICS lines"""
        log_files = list(self.test_dir.glob("*.log"))

        for log_file in log_files:
            with open(log_file) as f:
                for line in f:
                    if "METRICS:" in line:
                        # Extract JSON after "METRICS: "
                        match = re.search(r'METRICS:\s*({.*})', line)
                        if match:
                            try:
                                metric = json.loads(match.group(1))
                                self.metrics.append(metric)
                            except json.JSONDecodeError:
                                continue

    def analyze(self) -> Dict:
        """Analyze collected metrics"""
        if not self.metrics:
            return {
                "test_name": self.test_name,
                "error": "No metrics found"
            }

        # Separate by event type
        doc_received = [m for m in self.metrics if m.get("event_type") == "DocumentReceived"]
        doc_acked = [m for m in self.metrics if m.get("event_type") == "DocumentAcknowledged"]

        # Extract latencies
        latencies = [m["latency_ms"] for m in doc_received if "latency_ms" in m]

        # Count documents per node
        docs_by_node = defaultdict(int)
        for m in doc_received:
            node_id = m.get("node_id", "unknown")
            docs_by_node[node_id] += 1

        result = {
            "test_name": self.test_name,
            "total_docs_received": len(doc_received),
            "total_docs_acked": len(doc_acked),
            "node_count": len(docs_by_node),
            "docs_per_node": dict(docs_by_node),
        }

        # Latency stats
        if latencies:
            result["latency"] = {
                "min_ms": min(latencies),
                "max_ms": max(latencies),
                "avg_ms": statistics.mean(latencies),
                "median_ms": statistics.median(latencies),
                "p90_ms": statistics.quantiles(latencies, n=10)[8] if len(latencies) >= 10 else max(latencies),
                "p95_ms": statistics.quantiles(latencies, n=20)[18] if len(latencies) >= 20 else max(latencies),
            }

        return result


def analyze_test_suite(suite_dir: Path, suite_name: str) -> Dict:
    """Analyze all tests in a suite"""
    results = {
        "suite_name": suite_name,
        "tests": []
    }

    # Find all test subdirectories
    test_dirs = [d for d in suite_dir.iterdir() if d.is_dir()]
    test_dirs.sort()

    for test_dir in test_dirs:
        analyzer = E13TestAnalyzer(test_dir, test_dir.name)
        test_result = analyzer.analyze()
        results["tests"].append(test_result)

    return results


def print_summary(e13v2_results: Dict, e13v3_results: Dict):
    """Print comparison summary"""
    print("=" * 80)
    print("E13 Delta Sync Validation - Results Summary")
    print("=" * 80)
    print()

    # E13v2 Summary
    print("## E13v2: P2P Mesh (Limited Connections)")
    print()
    for test in e13v2_results["tests"]:
        if "error" in test:
            print(f"  {test['test_name']}: {test['error']}")
            continue

        print(f"### {test['test_name']}")
        print(f"  - Nodes: {test['node_count']}")
        print(f"  - Docs Received: {test['total_docs_received']}")
        print(f"  - Docs Acknowledged: {test['total_docs_acked']}")

        if "latency" in test:
            lat = test["latency"]
            print(f"  - Latency (ms):")
            print(f"      Min: {lat['min_ms']:.2f}")
            print(f"      Avg: {lat['avg_ms']:.2f}")
            print(f"      Median: {lat['median_ms']:.2f}")
            print(f"      P90: {lat['p90_ms']:.2f}")
            print(f"      P95: {lat['p95_ms']:.2f}")
            print(f"      Max: {lat['max_ms']:.2f}")
        print()

    # E13v3 Summary
    print("## E13v3: Mode 4 Hierarchical Aggregation")
    print()
    for test in e13v3_results["tests"]:
        if "error" in test:
            print(f"  {test['test_name']}: {test['error']}")
            continue

        print(f"### {test['test_name']}")
        print(f"  - Nodes: {test['node_count']}")
        print(f"  - Docs Received: {test['total_docs_received']}")
        print(f"  - Docs Acknowledged: {test['total_docs_acked']}")

        if "latency" in test:
            lat = test["latency"]
            print(f"  - Latency (ms):")
            print(f"      Min: {lat['min_ms']:.2f}")
            print(f"      Avg: {lat['avg_ms']:.2f}")
            print(f"      Median: {lat['median_ms']:.2f}")
            print(f"      P90: {lat['p90_ms']:.2f}")
            print(f"      P95: {lat['p95_ms']:.2f}")
            print(f"      Max: {lat['max_ms']:.2f}")
        print()

    # Comparison table
    print("=" * 80)
    print("## Latency Comparison: E13v2 vs E13v3")
    print("=" * 80)
    print()
    print("| Scale | Architecture | Nodes | Docs | Lat Avg (ms) | Lat P90 (ms) | Lat P95 (ms) |")
    print("|-------|--------------|-------|------|--------------|--------------|--------------|")

    # Extract scale-matched pairs
    for v2_test in e13v2_results["tests"]:
        if "error" in v2_test or "latency" not in v2_test:
            continue

        # Parse scale from test name
        match = re.search(r'(\d+)node', v2_test['test_name'])
        if not match:
            continue
        scale = match.group(1)

        # Find matching E13v3 test
        v3_test = None
        for test in e13v3_results["tests"]:
            if f"{scale}node" in test['test_name'] and "latency" in test:
                v3_test = test
                break

        # Print E13v2 row
        v2_lat = v2_test["latency"]
        print(f"| {scale}node | P2P Mesh | {v2_test['node_count']} | {v2_test['total_docs_received']} | "
              f"{v2_lat['avg_ms']:.2f} | {v2_lat['p90_ms']:.2f} | {v2_lat['p95_ms']:.2f} |")

        # Print E13v3 row if found
        if v3_test:
            v3_lat = v3_test["latency"]
            print(f"| {scale}node | Mode 4 Hier | {v3_test['node_count']} | {v3_test['total_docs_received']} | "
                  f"{v3_lat['avg_ms']:.2f} | {v3_lat['p90_ms']:.2f} | {v3_lat['p95_ms']:.2f} |")

    print()


def main():
    # Locate test results
    scripts_dir = Path(__file__).parent

    e13v2_dir = scripts_dir / "e13v2-full-matrix-20251114-114826"
    e13v3_dir = scripts_dir / "e13v3-mode4-hierarchical-20251114-121905"

    if not e13v2_dir.exists():
        print(f"Error: E13v2 results not found at {e13v2_dir}")
        sys.exit(1)

    if not e13v3_dir.exists():
        print(f"Error: E13v3 results not found at {e13v3_dir}")
        sys.exit(1)

    # Analyze both suites
    e13v2_results = analyze_test_suite(e13v2_dir, "E13v2 P2P Mesh")
    e13v3_results = analyze_test_suite(e13v3_dir, "E13v3 Mode 4 Hierarchical")

    # Print summary
    print_summary(e13v2_results, e13v3_results)

    # Save JSON results
    output_file = scripts_dir / "e13-delta-sync-analysis.json"
    with open(output_file, "w") as f:
        json.dump({
            "e13v2": e13v2_results,
            "e13v3": e13v3_results
        }, f, indent=2)

    print(f"✓ Detailed results saved to {output_file}")


if __name__ == "__main__":
    main()
