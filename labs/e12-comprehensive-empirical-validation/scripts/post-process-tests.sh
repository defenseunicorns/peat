#!/bin/bash

# Post-process test results to generate summary files
# Usage: ./post-process-tests.sh <test-dir1> [test-dir2] ...

for test_dir in "$@"; do
    if [ ! -d "$test_dir" ]; then
        echo "Skipping non-existent directory: $test_dir"
        continue
    fi

    echo "Processing: $test_dir"

    # Extract metrics from logs
    echo "  - Extracting metrics..."
    cat "$test_dir"/logs/*.log 2>/dev/null | grep "METRICS:" | sed 's/.*METRICS: //' > "$test_dir/all-metrics.jsonl"

    # Process Docker stats
    echo "  - Processing Docker stats..."
    python3 -c "
import json
import sys
from pathlib import Path
from collections import defaultdict

output_dir = Path('$test_dir')
stats_dir = output_dir / 'docker-stats'

if not stats_dir.exists():
    print(f'Warning: No docker-stats directory in {output_dir}')
    sys.exit(0)

# Aggregate stats across all collection points
node_stats = defaultdict(lambda: {
    'net_input_bytes': [],
    'net_output_bytes': [],
    'cpu_percent': [],
    'mem_usage_bytes': []
})

for stats_file in sorted(stats_dir.glob('stats-*.json')):
    try:
        with open(stats_file) as f:
            # Stats files are JSONL format (one JSON object per line)
            for line in f:
                line = line.strip()
                if not line:
                    continue

                container = json.loads(line)
                name = container.get('Name', 'unknown')

                # Parse network I/O (format: '1.23MB / 4.56MB')
                net_io = container.get('NetIO', '0B / 0B')
                if ' / ' in net_io:
                    input_str, output_str = net_io.split(' / ')

                    def parse_bytes(s):
                        s = s.strip()
                        # Check longest suffixes first to avoid matching 'B' in 'kB'
                        if 'GB' in s:
                            return float(s.replace('GB', '').replace('GiB', '')) * 1e9
                        elif 'MB' in s or 'MiB' in s:
                            return float(s.replace('MB', '').replace('MiB', '')) * 1e6
                        elif 'kB' in s or 'KiB' in s or 'KB' in s:
                            return float(s.replace('kB', '').replace('KiB', '').replace('KB', '')) * 1e3
                        elif 'B' in s:
                            return float(s.replace('B', ''))
                        return 0

                    node_stats[name]['net_input_bytes'].append(parse_bytes(input_str))
                    node_stats[name]['net_output_bytes'].append(parse_bytes(output_str))

                # Parse CPU percentage (format: '12.34%')
                cpu_str = container.get('CPUPerc', '0%').replace('%', '')
                try:
                    node_stats[name]['cpu_percent'].append(float(cpu_str))
                except:
                    pass

                # Parse memory usage (format: '123.4MiB / 456.7MiB')
                mem_usage = container.get('MemUsage', '0B / 0B')
                if ' / ' in mem_usage:
                    usage_str = mem_usage.split(' / ')[0].strip()
                    node_stats[name]['mem_usage_bytes'].append(parse_bytes(usage_str))

    except Exception as e:
        continue

# Calculate aggregates and write summary
summary = {}
for node, stats in node_stats.items():
    if stats['net_input_bytes']:
        # Network: use max values (cumulative counters)
        max_input = max(stats['net_input_bytes'])
        max_output = max(stats['net_output_bytes'])

        # CPU/Memory: use average
        avg_cpu = sum(stats['cpu_percent']) / len(stats['cpu_percent']) if stats['cpu_percent'] else 0
        avg_mem = sum(stats['mem_usage_bytes']) / len(stats['mem_usage_bytes']) if stats['mem_usage_bytes'] else 0

        summary[node] = {
            'net_input_bytes': int(max_input),
            'net_output_bytes': int(max_output),
            'net_total_bytes': int(max_input + max_output),
            'avg_cpu_percent': round(avg_cpu, 2),
            'avg_mem_bytes': int(avg_mem)
        }

# Write summary
with open(output_dir / 'docker-stats-summary.json', 'w') as f:
    json.dump(summary, f, indent=2)

# Write human-readable report
with open(output_dir / 'docker-stats-summary.txt', 'w') as f:
    f.write('Docker Network Statistics Summary\n')
    f.write('=' * 80 + '\n\n')

    total_input = sum(s['net_input_bytes'] for s in summary.values())
    total_output = sum(s['net_output_bytes'] for s in summary.values())
    total_combined = total_input + total_output

    f.write(f'Total Network I/O:\n')
    f.write(f'  Input:  {total_input:,} bytes ({total_input/1e6:.2f} MB)\n')
    f.write(f'  Output: {total_output:,} bytes ({total_output/1e6:.2f} MB)\n')
    f.write(f'  Total:  {total_combined:,} bytes ({total_combined/1e6:.2f} MB)\n\n')

    f.write(f'Per-Node Breakdown:\n')
    f.write('-' * 80 + '\n')

    for node in sorted(summary.keys()):
        s = summary[node]
        f.write(f'\n{node}:\n')
        f.write(f\"  Input:  {s['net_input_bytes']:,} bytes ({s['net_input_bytes']/1e6:.2f} MB)\n\")
        f.write(f\"  Output: {s['net_output_bytes']:,} bytes ({s['net_output_bytes']/1e6:.2f} MB)\n\")
        f.write(f\"  Total:  {s['net_total_bytes']:,} bytes ({s['net_total_bytes']/1e6:.2f} MB)\n\")
        f.write(f\"  CPU:    {s['avg_cpu_percent']:.2f}%\n\")
        f.write(f\"  Memory: {s['avg_mem_bytes']:,} bytes ({s['avg_mem_bytes']/1e6:.2f} MB)\n\")
"

    # Process application metrics
    echo "  - Processing application metrics..."
    python3 -c "
import json
import sys
from pathlib import Path
from collections import Counter

output_dir = Path('$test_dir')
metrics_file = output_dir / 'all-metrics.jsonl'

if not metrics_file.exists():
    print(f'Warning: No all-metrics.jsonl in {output_dir}')
    sys.exit(0)

# Parse all metrics
events = []
with open(metrics_file) as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        try:
            events.append(json.loads(line))
        except:
            continue

if not events:
    print(f'Warning: No metrics found in {metrics_file}')
    sys.exit(0)

# Calculate summary statistics
event_types = Counter(e.get('event_type') for e in events)

# Aggregate metrics by type
summary = {
    'total_events': len(events),
    'event_type_counts': dict(event_types),
    'sync_metrics': {},
    'latency_metrics': {}
}

# Extract sync-related metrics
sync_events = [e for e in events if e.get('event_type') in ['MessageSent', 'MessageReceived']]
if sync_events:
    total_bytes = sum(e.get('message_size_bytes', 0) for e in sync_events if 'message_size_bytes' in e)
    summary['sync_metrics']['total_app_bytes'] = total_bytes
    summary['sync_metrics']['message_count'] = len(sync_events)

# Extract latency metrics
latency_events = [e for e in events if 'latency_us' in e]
if latency_events:
    latencies = [e['latency_us'] for e in latency_events]
    latencies.sort()
    summary['latency_metrics']['count'] = len(latencies)
    summary['latency_metrics']['p50_us'] = latencies[len(latencies) // 2]
    summary['latency_metrics']['p90_us'] = latencies[int(len(latencies) * 0.9)]
    summary['latency_metrics']['p99_us'] = latencies[int(len(latencies) * 0.99)]

# Extract document reception metrics
doc_receptions = [e for e in events if e.get('event_type') == 'DocumentReceived']
summary['doc_receptions'] = len(doc_receptions)

# Write summary
with open(output_dir / 'test-summary.json', 'w') as f:
    json.dump(summary, f, indent=2)

# Write human-readable report
with open(output_dir / 'test-summary.txt', 'w') as f:
    f.write('Application Metrics Summary\n')
    f.write('=' * 80 + '\n\n')

    f.write(f\"Total Events: {summary['total_events']}\n\n\")

    f.write('Event Type Breakdown:\n')
    for event_type, count in sorted(summary['event_type_counts'].items()):
        f.write(f'  {event_type}: {count}\n')

    f.write('\nSync Metrics:\n')
    if summary['sync_metrics']:
        sm = summary['sync_metrics']
        f.write(f\"  Total Application Bytes: {sm.get('total_app_bytes', 0):,}\n\")
        f.write(f\"  Message Count: {sm.get('message_count', 0)}\n\")
    else:
        f.write('  No sync metrics found\n')

    f.write('\nLatency Metrics:\n')
    if summary['latency_metrics']:
        lm = summary['latency_metrics']
        f.write(f\"  Count: {lm.get('count', 0)}\n\")
        f.write(f\"  P50: {lm.get('p50_us', 0)/1000:.1f} ms\n\")
        f.write(f\"  P90: {lm.get('p90_us', 0)/1000:.1f} ms\n\")
        f.write(f\"  P99: {lm.get('p99_us', 0)/1000:.1f} ms\n\")
    else:
        f.write('  No latency metrics found\n')

    f.write(f\"\nDocument Receptions: {summary.get('doc_receptions', 0)}\n\")
"

    # Create test-config.txt from test-config.json
    if [ -f "$test_dir/test-config.json" ]; then
        echo "  - Converting test config..."
        python3 -c "
import json
from pathlib import Path

config_file = Path('$test_dir') / 'test-config.json'
output_file = Path('$test_dir') / 'test-config.txt'

with open(config_file) as f:
    config = json.load(f)

with open(output_file, 'w') as f:
    for key, value in config.items():
        f.write(f'{key}: {value}\n')
"
    fi

    echo "  ✓ Complete"
done

echo "Post-processing complete"
