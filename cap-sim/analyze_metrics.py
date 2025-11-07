#!/usr/bin/env python3
"""
Parse METRICS JSON from container logs and calculate quantitative analysis.

This script extracts:
- Convergence time (time from insert to last reader receiving document)
- Latency distribution (p50, p90, p99)
- Per-node latency statistics
- Traffic analysis (messages/sec, bandwidth per node type)
"""

import json
import sys
import statistics
from typing import List, Dict, Any

def parse_metrics(log_file: str) -> Dict[str, Any]:
    """Parse METRICS JSON lines from a log file."""
    metrics = {
        'inserts': [],
        'receives': [],
        'messages': [],
        'acknowledgments': [],
        'all_acks_received': []
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
                elif event_type == 'MessageSent':
                    metrics['messages'].append(event)
                elif event_type == 'DocumentAcknowledged':
                    metrics['acknowledgments'].append(event)
                elif event_type == 'AllAcksReceived':
                    metrics['all_acks_received'].append(event)
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

def analyze_traffic(messages: List[Dict]) -> Dict[str, Any]:
    """Analyze traffic patterns by node type."""
    if not messages:
        return {
            'by_node_type': {},
            'total': {
                'messages': 0,
                'total_bytes': 0,
                'duration_sec': 0,
                'messages_per_sec': 0,
                'bandwidth_kbps': 0
            }
        }

    # Group messages by node type
    by_type = {}
    for msg in messages:
        node_type = msg.get('node_type', 'unknown')
        if node_type not in by_type:
            by_type[node_type] = []
        by_type[node_type].append(msg)

    # Calculate stats per node type
    type_stats = {}
    for node_type, type_messages in by_type.items():
        if not type_messages:
            continue

        # Get time range
        timestamps = [m['timestamp_us'] for m in type_messages]
        min_time = min(timestamps)
        max_time = max(timestamps)
        duration_us = max_time - min_time
        duration_sec = duration_us / 1_000_000.0 if duration_us > 0 else 1.0

        # Calculate totals
        total_messages = len(type_messages)
        total_bytes = sum(m['message_size_bytes'] for m in type_messages)

        # Calculate rates
        messages_per_sec = total_messages / duration_sec if duration_sec > 0 else 0
        bytes_per_sec = total_bytes / duration_sec if duration_sec > 0 else 0
        bandwidth_kbps = (bytes_per_sec * 8) / 1000.0  # Convert to kilobits per second

        # Count unique nodes
        unique_nodes = len(set(m['node_id'] for m in type_messages))

        type_stats[node_type] = {
            'total_messages': total_messages,
            'total_bytes': total_bytes,
            'duration_sec': round(duration_sec, 2),
            'messages_per_sec': round(messages_per_sec, 2),
            'bytes_per_sec': round(bytes_per_sec, 2),
            'bandwidth_kbps': round(bandwidth_kbps, 2),
            'avg_message_size': round(total_bytes / total_messages, 2) if total_messages > 0 else 0,
            'unique_nodes': unique_nodes
        }

    # Calculate overall totals
    all_timestamps = [m['timestamp_us'] for m in messages]
    total_duration_us = max(all_timestamps) - min(all_timestamps)
    total_duration_sec = total_duration_us / 1_000_000.0 if total_duration_us > 0 else 1.0
    total_messages = len(messages)
    total_bytes = sum(m['message_size_bytes'] for m in messages)
    total_msg_per_sec = total_messages / total_duration_sec if total_duration_sec > 0 else 0
    total_bytes_per_sec = total_bytes / total_duration_sec if total_duration_sec > 0 else 0
    total_bandwidth_kbps = (total_bytes_per_sec * 8) / 1000.0

    return {
        'by_node_type': type_stats,
        'total': {
            'messages': total_messages,
            'total_bytes': total_bytes,
            'duration_sec': round(total_duration_sec, 2),
            'messages_per_sec': round(total_msg_per_sec, 2),
            'bytes_per_sec': round(total_bytes_per_sec, 2),
            'bandwidth_kbps': round(total_bandwidth_kbps, 2)
        }
    }

def analyze_acknowledgments(acks: List[Dict], all_acks_received: List[Dict]) -> Dict[str, Any]:
    """Analyze round-trip acknowledgment latency."""
    if not all_acks_received:
        return {
            'round_trip_latency_ms': 0,
            'ack_count': 0,
            'acknowledgments': []
        }

    # There should be only one AllAcksReceived event (from the writer)
    ack_event = all_acks_received[0] if all_acks_received else None

    if not ack_event:
        return {
            'round_trip_latency_ms': 0,
            'ack_count': 0,
            'acknowledgments': []
        }

    return {
        'round_trip_latency_ms': ack_event.get('round_trip_latency_ms', 0),
        'ack_count': ack_event.get('ack_count', 0),
        'acknowledgments': [{'node_id': a['node_id'], 'doc_id': a['doc_id']} for a in acks]
    }

def main():
    if len(sys.argv) < 2:
        print("Usage: analyze_metrics.py <log_file1> [log_file2 ...]", file=sys.stderr)
        sys.exit(1)

    all_inserts = []
    all_receives = []
    all_messages = []
    all_acknowledgments = []
    all_acks_received = []

    # Parse all log files
    for log_file in sys.argv[1:]:
        try:
            metrics = parse_metrics(log_file)
            all_inserts.extend(metrics['inserts'])
            all_receives.extend(metrics['receives'])
            all_messages.extend(metrics['messages'])
            all_acknowledgments.extend(metrics['acknowledgments'])
            all_acks_received.extend(metrics['all_acks_received'])
        except FileNotFoundError:
            print(f"Warning: File not found: {log_file}", file=sys.stderr)
            continue

    # Calculate latency statistics
    latencies = [r['latency_ms'] for r in all_receives]
    latency_stats = calculate_statistics(latencies)

    # Calculate convergence time
    convergence = analyze_convergence(all_inserts, all_receives)

    # Calculate traffic statistics
    traffic = analyze_traffic(all_messages)

    # Calculate acknowledgment statistics
    acknowledgments = analyze_acknowledgments(all_acknowledgments, all_acks_received)

    # Output as JSON for easy parsing
    result = {
        'latency': latency_stats,
        'convergence': convergence,
        'traffic': traffic,
        'acknowledgments': acknowledgments,
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
