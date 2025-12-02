#!/usr/bin/env python3
"""Quick analysis of E13v3 Mode 4 hierarchical results"""

import json
import sys
import statistics
from pathlib import Path

def analyze_logs(log_dir):
    latencies = []

    # Find all log files
    for log_file in Path(log_dir).glob("*.log"):
        with open(log_file) as f:
            for line in f:
                if "METRICS" in line and "DocumentReceived" in line:
                    try:
                        # Extract JSON from log line
                        json_start = line.find('{"event_type"')
                        if json_start == -1:
                            continue
                        data = json.loads(line[json_start:].strip())
                        latencies.append(data['latency_ms'])
                    except Exception as e:
                        continue

    if not latencies:
        print("No latency data found")
        return

    latencies.sort()
    n = len(latencies)

    p50_idx = int(n * 0.50)
    p90_idx = int(n * 0.90)
    p95_idx = int(n * 0.95)
    p99_idx = int(n * 0.99)

    print(f"E13v3 Mode 4 Hierarchical - 24 nodes @ 1Gbps")
    print(f"Total events: {n}")
    print(f"Mean: {statistics.mean(latencies):.1f} ms")
    print(f"Median (P50): {latencies[p50_idx]:.1f} ms")
    print(f"P90: {latencies[p90_idx]:.1f} ms")
    print(f"P95: {latencies[p95_idx]:.1f} ms")
    print(f"P99: {latencies[p99_idx]:.1f} ms")
    print(f"Max: {max(latencies):.1f} ms")
    print(f"Min: {min(latencies):.1f} ms")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: quick-analyze-e13v3.py <log_directory>")
        sys.exit(1)

    analyze_logs(sys.argv[1])
