#!/usr/bin/env python3
"""
Parse METRICS JSON from container logs and calculate quantitative analysis.

This script extracts:
- Convergence time (time from insert to last reader receiving document)
- Latency distribution (p50, p90, p99)
- Per-node latency statistics
"""

import json
import sys
import statistics
from typing import List, Dict, Any

def parse_metrics(log_file: str) -> Dict[str, Any]:
    """Parse METRICS JSON lines from a log file."""
    metrics = {
        'inserts': [],
        'receives': []
    }

    with open(log_file, 'r') as f:
        for line in f:
            if 'METRICS:' not in line:
                continue

            # Extract JSON after "METRICS: "
            json_start = line.find('METRICS:') + 8
            json_str = line[json_start:].strip()

            try:
                event = json.loads(json_str)
                event_type = event.get('event_type')

                if event_type == 'DocumentInserted':
                    metrics['inserts'].append(event)
                elif event_type == 'DocumentReceived':
                    metrics['receives'].append(event)
            except json.JSONDecodeError as e:
                print(f"Warning: Failed to parse JSON: {json_str}", file=sys.stderr)
                continue

    return metrics

def calculate_statistics(latencies: List[float]) -> Dict[str, float]:
    """Calculate percentile statistics from latency values."""
    if not latencies:
        return {
            'count': 0,
            'min': 0,
            'max': 0,
            'mean': 0,
            'median': 0,
            'p90': 0,
            'p95': 0,
            'p99': 0
        }

    sorted_latencies = sorted(latencies)

    return {
        'count': len(latencies),
        'min': min(latencies),
        'max': max(latencies),
        'mean': statistics.mean(latencies),
        'median': statistics.median(latencies),
        'p90': statistics.quantiles(sorted_latencies, n=100)[89] if len(latencies) >= 10 else sorted_latencies[-1],
        'p95': statistics.quantiles(sorted_latencies, n=100)[94] if len(latencies) >= 20 else sorted_latencies[-1],
        'p99': statistics.quantiles(sorted_latencies, n=100)[98] if len(latencies) >= 100 else sorted_latencies[-1]
    }

def analyze_convergence(inserts: List[Dict], receives: List[Dict]) -> Dict[str, Any]:
    """Analyze convergence time from insert to all nodes receiving."""
    if not inserts or not receives:
        return {
            'insert_time_us': 0,
            'first_receive_time_us': 0,
            'last_receive_time_us': 0,
            'convergence_time_ms': 0,
            'nodes_received': 0
        }

    # Get insert timestamp (should be only one)
    insert_time_us = inserts[0]['timestamp_us']

    # Get all receive times
    receive_times = [r['received_at_us'] for r in receives]

    if not receive_times:
        return {
            'insert_time_us': insert_time_us,
            'first_receive_time_us': 0,
            'last_receive_time_us': 0,
            'convergence_time_ms': 0,
            'nodes_received': 0
        }

    first_receive = min(receive_times)
    last_receive = max(receive_times)
    convergence_us = last_receive - insert_time_us

    return {
        'insert_time_us': insert_time_us,
        'first_receive_time_us': first_receive,
        'last_receive_time_us': last_receive,
        'convergence_time_ms': convergence_us / 1000.0,
        'first_node_latency_ms': (first_receive - insert_time_us) / 1000.0,
        'nodes_received': len(receive_times)
    }

def main():
    if len(sys.argv) < 2:
        print("Usage: analyze_metrics.py <log_file1> [log_file2 ...]", file=sys.stderr)
        sys.exit(1)

    all_inserts = []
    all_receives = []

    # Parse all log files
    for log_file in sys.argv[1:]:
        try:
            metrics = parse_metrics(log_file)
            all_inserts.extend(metrics['inserts'])
            all_receives.extend(metrics['receives'])
        except FileNotFoundError:
            print(f"Warning: File not found: {log_file}", file=sys.stderr)
            continue

    # Calculate latency statistics
    latencies = [r['latency_ms'] for r in all_receives]
    latency_stats = calculate_statistics(latencies)

    # Calculate convergence time
    convergence = analyze_convergence(all_inserts, all_receives)

    # Output as JSON for easy parsing
    result = {
        'latency': latency_stats,
        'convergence': convergence,
        'per_node': []
    }

    # Per-node breakdown
    nodes = {}
    for receive in all_receives:
        node_id = receive['node_id']
        if node_id not in nodes:
            nodes[node_id] = []
        nodes[node_id].append(receive['latency_ms'])

    for node_id, node_latencies in sorted(nodes.items()):
        result['per_node'].append({
            'node_id': node_id,
            'latency_ms': node_latencies[0] if node_latencies else 0,
            'count': len(node_latencies)
        })

    print(json.dumps(result, indent=2))

if __name__ == '__main__':
    main()
