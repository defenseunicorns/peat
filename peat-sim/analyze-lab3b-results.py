#!/usr/bin/env python3
"""
Analyze Lab 3b Results: Flat Mesh with HIVE CRDT

Compares Lab 3b (P2P + CRDT) to Lab 3 (P2P raw TCP) to measure CRDT overhead.
"""

import sys
import json
import glob
from pathlib import Path
from collections import defaultdict
from statistics import mean, median


def parse_metrics_from_log(log_file):
    """Extract metrics from container log file."""
    metrics = {
        'documents_inserted': [],
        'crdt_sync_latencies': [],
        'updates_published': 0,
        'coordinator_initialized': False,
        'hierarchy_level': None,
        'peer_count': 0,
    }

    try:
        with open(log_file, 'r') as f:
            for line in f:
                # Check for flat mesh initialization
                if 'FLAT MESH MODE' in line:
                    metrics['coordinator_initialized'] = True

                # Check hierarchy level
                if 'Initialized as flat mesh peer at level:' in line:
                    if 'Squad' in line:
                        metrics['hierarchy_level'] = 'Squad'

                # Count published updates
                if 'Published state update' in line:
                    metrics['updates_published'] += 1

                # Extract METRICS events
                if 'METRICS:' in line:
                    try:
                        json_start = line.index('METRICS:') + 8
                        metric_data = json.loads(line[json_start:].strip())

                        if metric_data.get('event_type') == 'DocumentInserted':
                            metrics['documents_inserted'].append(metric_data)
                    except (json.JSONDecodeError, ValueError):
                        pass

                # Extract peer count from role updates
                if 'Current role:' in line and 'peers' in line:
                    try:
                        parts = line.split('peers')
                        if parts:
                            count_str = parts[0].split()[-1]
                            metrics['peer_count'] = max(metrics['peer_count'], int(count_str))
                    except ValueError:
                        pass

    except FileNotFoundError:
        pass

    return metrics


def analyze_lab3b_results(results_dir):
    """Analyze Lab 3b test results."""

    print("=" * 80)
    print("LAB 3B ANALYSIS: P2P Flat Mesh with HIVE CRDT")
    print("=" * 80)
    print()

    log_dir = Path(results_dir) / "logs"
    if not log_dir.exists():
        print(f"❌ Log directory not found: {log_dir}")
        return

    # Find all peer logs
    log_files = sorted(glob.glob(str(log_dir / "peer-*.log")))

    if not log_files:
        print(f"❌ No peer logs found in {log_dir}")
        return

    print(f"Found {len(log_files)} peer logs")
    print()

    # Parse metrics from each peer
    all_metrics = {}
    for log_file in log_files:
        peer_id = Path(log_file).stem
        all_metrics[peer_id] = parse_metrics_from_log(log_file)

    # Summary statistics
    print("FLAT MESH COORDINATION")
    print("-" * 80)

    initialized_count = sum(1 for m in all_metrics.values() if m['coordinator_initialized'])
    print(f"Nodes initialized in flat mesh mode: {initialized_count}/{len(log_files)}")

    squad_level_count = sum(1 for m in all_metrics.values() if m['hierarchy_level'] == 'Squad')
    print(f"Nodes at Squad level: {squad_level_count}/{len(log_files)}")

    peer_counts = [m['peer_count'] for m in all_metrics.values() if m['peer_count'] > 0]
    if peer_counts:
        print(f"Peer visibility: {min(peer_counts)}-{max(peer_counts)} peers seen")

    print()

    # CRDT Publishing
    print("CRDT DOCUMENT PUBLISHING")
    print("-" * 80)

    for peer_id, metrics in sorted(all_metrics.items()):
        updates = metrics['updates_published']
        documents = len(metrics['documents_inserted'])
        print(f"{peer_id:12s}: {updates:3d} updates published, {documents:3d} documents inserted")

    print()

    # Overall statistics
    total_updates = sum(m['updates_published'] for m in all_metrics.values())
    total_documents = sum(len(m['documents_inserted']) for m in all_metrics.values())

    print("OVERALL STATISTICS")
    print("-" * 80)
    print(f"Total updates published:    {total_updates}")
    print(f"Total documents inserted:   {total_documents}")
    print(f"Average per node:           {total_updates / len(log_files):.1f} updates")

    print()

    # Success criteria
    print("SUCCESS CRITERIA")
    print("-" * 80)

    checks = []

    # Check 1: All nodes initialized
    if initialized_count == len(log_files):
        print("✅ All nodes initialized in flat mesh mode")
        checks.append(True)
    else:
        print(f"❌ Only {initialized_count}/{len(log_files)} nodes initialized")
        checks.append(False)

    # Check 2: All at Squad level
    if squad_level_count == len(log_files):
        print("✅ All nodes at Squad hierarchy level")
        checks.append(True)
    else:
        print(f"⚠️  Only {squad_level_count}/{len(log_files)} at Squad level")
        checks.append(False)

    # Check 3: Documents published
    if total_updates > 0:
        print(f"✅ CRDT documents published ({total_updates} total)")
        checks.append(True)
    else:
        print("❌ No CRDT documents published")
        checks.append(False)

    print()

    if all(checks):
        print("🎉 Lab 3b validation PASSED!")
        print()
        print("Next steps:")
        print("  1. Compare to Lab 3 results to measure CRDT overhead")
        print("  2. Run full Lab 3b test suite with multiple node counts")
        print("  3. Proceed to Lab 4 (hierarchical CRDT)")
        return 0
    else:
        print("❌ Lab 3b validation FAILED - see issues above")
        return 1


def compare_to_lab3(lab3b_dir, lab3_dir):
    """Compare Lab 3b (CRDT) to Lab 3 (raw TCP) to measure overhead."""

    print()
    print("=" * 80)
    print("COMPARISON: Lab 3 (Raw TCP) vs Lab 3b (HIVE CRDT)")
    print("=" * 80)
    print()

    # TODO: Implement comparison logic once we have both result sets
    print("📊 Comparison analysis not yet implemented")
    print()
    print("To compare:")
    print(f"  Lab 3b results: {lab3b_dir}")
    print(f"  Lab 3 results:  {lab3_dir}")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 analyze-lab3b-results.py <results_dir> [lab3_results_dir]")
        print()
        print("Example:")
        print("  python3 analyze-lab3b-results.py lab3b-validation-20251123-123456")
        print("  python3 analyze-lab3b-results.py lab3b-validation-20251123-123456 p2p-mesh-comprehensive-20251122-202726")
        sys.exit(1)

    results_dir = sys.argv[1]
    exit_code = analyze_lab3b_results(results_dir)

    if len(sys.argv) > 2:
        lab3_dir = sys.argv[2]
        compare_to_lab3(results_dir, lab3_dir)

    sys.exit(exit_code)
