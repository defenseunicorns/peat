#!/usr/bin/env python3
"""
E12 Comprehensive Empirical Validation Analysis
Analyzes 30 test scenarios across 3 architectures, multiple scales, and bandwidth constraints.
"""

import json
import os
from pathlib import Path
from collections import defaultdict
import statistics

def parse_test_name(test_dir):
    """Parse test directory name into components"""
    parts = test_dir.split('-')

    if parts[0] == 'traditional':
        arch = 'Traditional IoT'
        scale = parts[1]
        bw = parts[2]
    elif parts[0] == 'cap':
        if parts[1] == 'full':
            arch = 'CAP Full Mesh'
            scale = parts[2]
            bw = parts[3]
        elif parts[1] == 'hierarchical':
            arch = 'CAP Hierarchical'
            scale = parts[2]
            bw = parts[3]
        else:
            arch = f'CAP-{parts[1]}'
            scale = parts[2]
            bw = parts[3]
    else:
        arch = parts[0]
        scale = parts[1]
        bw = parts[2]

    # Extract node count
    node_count = int(scale.replace('node', ''))

    # Map bandwidth to numeric value for sorting
    bw_map = {
        '1gbps': 1000,
        '100mbps': 100,
        '1mbps': 1,
        '256kbps': 0.256
    }
    bw_value = bw_map.get(bw, 0)

    return {
        'architecture': arch,
        'scale': scale,
        'node_count': node_count,
        'bandwidth': bw,
        'bandwidth_mbps': bw_value
    }

def load_test_results(results_dir):
    """Load all test results from the results directory"""
    results = []

    results_path = Path(results_dir)
    for test_dir in sorted(results_path.iterdir()):
        if not test_dir.is_dir():
            continue

        summary_file = test_dir / 'test-summary.json'
        if not summary_file.exists():
            print(f"Warning: No summary file in {test_dir.name}")
            continue

        with open(summary_file) as f:
            summary = json.load(f)

        test_info = parse_test_name(test_dir.name)
        test_info['test_name'] = test_dir.name
        test_info['metrics'] = summary

        # Load docker stats if available
        docker_stats_file = test_dir / 'docker-stats-summary.json'
        if docker_stats_file.exists():
            with open(docker_stats_file) as f:
                test_info['docker_stats'] = json.load(f)

        results.append(test_info)

    return results

def format_bandwidth(bw_mbps):
    """Format bandwidth value for display"""
    if bw_mbps >= 1000:
        return f"{int(bw_mbps/1000)} Gbps"
    elif bw_mbps >= 1:
        return f"{int(bw_mbps)} Mbps"
    else:
        return f"{int(bw_mbps*1000)} Kbps"

def analyze_by_architecture(results):
    """Group and analyze results by architecture"""
    by_arch = defaultdict(list)
    for test in results:
        by_arch[test['architecture']].append(test)

    print("\n" + "="*80)
    print("ANALYSIS BY ARCHITECTURE")
    print("="*80)

    for arch, tests in sorted(by_arch.items()):
        print(f"\n{arch}")
        print("-" * 60)
        print(f"Tests run: {len(tests)}")

        # Calculate aggregate statistics
        total_messages = sum(t['metrics'].get('message_sent_count', 0) for t in tests)
        total_docs = sum(t['metrics'].get('document_inserted_count', 0) for t in tests)

        latencies_avg = [t['metrics'].get('latency_avg_ms', 0) for t in tests if t['metrics'].get('latency_count', 0) > 0]
        latencies_p99 = [t['metrics'].get('latency_p99_ms', 0) for t in tests if t['metrics'].get('latency_count', 0) > 0]

        if latencies_avg:
            print(f"Average latency (mean):   {statistics.mean(latencies_avg):.2f} ms")
            print(f"Average latency (median): {statistics.median(latencies_avg):.2f} ms")
            print(f"P99 latency (mean):       {statistics.mean(latencies_p99):.2f} ms")
            print(f"P99 latency (median):     {statistics.median(latencies_p99):.2f} ms")

        print(f"Total messages sent:      {total_messages:,}")
        print(f"Total documents inserted: {total_docs:,}")

def analyze_by_scale(results):
    """Group and analyze results by scale"""
    by_scale = defaultdict(list)
    for test in results:
        by_scale[test['node_count']].append(test)

    print("\n" + "="*80)
    print("ANALYSIS BY SCALE")
    print("="*80)

    for scale, tests in sorted(by_scale.items()):
        print(f"\n{scale} Nodes")
        print("-" * 60)
        print(f"Tests run: {len(tests)}")

        # Group by architecture
        by_arch = defaultdict(list)
        for test in tests:
            by_arch[test['architecture']].append(test)

        for arch, arch_tests in sorted(by_arch.items()):
            latencies = [t['metrics'].get('latency_avg_ms', 0) for t in arch_tests if t['metrics'].get('latency_count', 0) > 0]
            if latencies:
                print(f"  {arch:25s} - Avg latency: {statistics.mean(latencies):7.2f} ms")

def analyze_by_bandwidth(results):
    """Group and analyze results by bandwidth constraint"""
    by_bw = defaultdict(list)
    for test in results:
        by_bw[test['bandwidth']].append(test)

    print("\n" + "="*80)
    print("ANALYSIS BY BANDWIDTH CONSTRAINT")
    print("="*80)

    for bw, tests in sorted(by_bw.items(), key=lambda x: x[1][0]['bandwidth_mbps'] if x[1] else 0, reverse=True):
        bw_formatted = format_bandwidth(tests[0]['bandwidth_mbps'])
        print(f"\n{bw_formatted}")
        print("-" * 60)
        print(f"Tests run: {len(tests)}")

        # Group by architecture
        by_arch = defaultdict(list)
        for test in tests:
            by_arch[test['architecture']].append(test)

        for arch, arch_tests in sorted(by_arch.items()):
            latencies = [t['metrics'].get('latency_avg_ms', 0) for t in arch_tests if t['metrics'].get('latency_count', 0) > 0]
            p99_latencies = [t['metrics'].get('latency_p99_ms', 0) for t in arch_tests if t['metrics'].get('latency_count', 0) > 0]
            if latencies:
                print(f"  {arch:25s} - Avg: {statistics.mean(latencies):7.2f} ms, P99: {statistics.mean(p99_latencies):7.2f} ms")

def create_comparison_table(results):
    """Create a detailed comparison table"""
    print("\n" + "="*80)
    print("DETAILED COMPARISON TABLE")
    print("="*80)
    print(f"\n{'Test':<40} {'Nodes':>6} {'BW':>8} {'Avg Lat':>9} {'P99 Lat':>9} {'Msgs':>8}")
    print("-" * 90)

    # Sort by architecture, then scale, then bandwidth
    sorted_results = sorted(results, key=lambda x: (x['architecture'], x['node_count'], -x['bandwidth_mbps']))

    current_arch = None
    for test in sorted_results:
        if current_arch != test['architecture']:
            if current_arch is not None:
                print()
            current_arch = test['architecture']
            print(f"\n{current_arch}:")

        m = test['metrics']
        bw_str = format_bandwidth(test['bandwidth_mbps'])

        lat_avg = m.get('latency_avg_ms', 0)
        lat_p99 = m.get('latency_p99_ms', 0)
        msg_count = m.get('message_sent_count', 0)

        print(f"  {test['test_name']:<38} {test['node_count']:>6} {bw_str:>8} {lat_avg:>8.2f}m {lat_p99:>8.2f}m {msg_count:>8}")

def generate_key_findings(results):
    """Generate key findings and insights"""
    print("\n" + "="*80)
    print("KEY FINDINGS")
    print("="*80)

    # Group by architecture for comparison
    by_arch = defaultdict(list)
    for test in results:
        by_arch[test['architecture']].append(test)

    # Finding 1: Compare latencies across architectures
    print("\n1. LATENCY COMPARISON ACROSS ARCHITECTURES")
    print("-" * 60)

    for arch, tests in sorted(by_arch.items()):
        latencies_avg = [t['metrics'].get('latency_avg_ms', 0) for t in tests if t['metrics'].get('latency_count', 0) > 0]
        latencies_p99 = [t['metrics'].get('latency_p99_ms', 0) for t in tests if t['metrics'].get('latency_count', 0) > 0]

        if latencies_avg:
            print(f"{arch:25s} - Mean Avg: {statistics.mean(latencies_avg):7.2f} ms, Mean P99: {statistics.mean(latencies_p99):9.2f} ms")

    # Finding 2: Scaling behavior
    print("\n2. SCALING BEHAVIOR")
    print("-" * 60)

    scales = sorted(set(t['node_count'] for t in results))
    for arch in sorted(by_arch.keys()):
        scale_lats = {}
        for scale in scales:
            scale_tests = [t for t in by_arch[arch] if t['node_count'] == scale]
            if scale_tests:
                lats = [t['metrics'].get('latency_avg_ms', 0) for t in scale_tests if t['metrics'].get('latency_count', 0) > 0]
                if lats:
                    scale_lats[scale] = statistics.mean(lats)

        if scale_lats:
            print(f"\n{arch}:")
            for scale in sorted(scale_lats.keys()):
                print(f"  {scale:3d} nodes: {scale_lats[scale]:7.2f} ms avg latency")

    # Finding 3: Bandwidth impact
    print("\n3. BANDWIDTH CONSTRAINT IMPACT")
    print("-" * 60)

    bandwidth_tests = {}
    for test in results:
        bw_key = test['bandwidth_mbps']
        if bw_key not in bandwidth_tests:
            bandwidth_tests[bw_key] = []
        bandwidth_tests[bw_key].append(test)

    for bw_mbps in sorted(bandwidth_tests.keys(), reverse=True):
        tests = bandwidth_tests[bw_mbps]
        latencies = [t['metrics'].get('latency_avg_ms', 0) for t in tests if t['metrics'].get('latency_count', 0) > 0]
        bw_str = format_bandwidth(bw_mbps)
        if latencies:
            print(f"{bw_str:10s} - Mean latency: {statistics.mean(latencies):7.2f} ms")

def main():
    results_dir = 'e12-comprehensive-results-20251116-085035'

    if not os.path.exists(results_dir):
        print(f"Error: Results directory '{results_dir}' not found")
        return

    print("Loading E12 Comprehensive Validation Results...")
    results = load_test_results(results_dir)
    print(f"Loaded {len(results)} test results")

    # Run analyses
    create_comparison_table(results)
    analyze_by_architecture(results)
    analyze_by_scale(results)
    analyze_by_bandwidth(results)
    generate_key_findings(results)

    print("\n" + "="*80)
    print("ANALYSIS COMPLETE")
    print("="*80)

if __name__ == '__main__':
    main()
