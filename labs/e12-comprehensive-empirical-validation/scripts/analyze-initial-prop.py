#!/usr/bin/env python3
import json
import sys
import statistics

latencies = []
for line in open('e13v2-full-matrix-20251113-113116/p2p-limited-96node-1gbps/clab-cap-battalion-p2p-limited-mesh-node-000.log'):
    if 'InitialPropagation' in line:
        try:
            start = line.find('{"event_type"')
            data = json.loads(line[start:])
            latencies.append(data['latency_ms'])
        except: pass

# Aggregate from all log files
import glob
for logfile in glob.glob('e13v2-full-matrix-20251113-113116/p2p-limited-96node-1gbps/*.log'):
    for line in open(logfile):
        if 'InitialPropagation' in line:
            try:
                start = line.find('{"event_type"')
                data = json.loads(line[start:])
                latencies.append(data['latency_ms'])
            except: pass

if latencies:
    latencies.sort()
    n = len(latencies)
    print(f'=== InitialPropagation Latencies (96-node P2P mesh) ===')
    print(f'Total events: {n}')
    print(f'Mean: {statistics.mean(latencies):.1f} ms')
    print(f'Median (P50): {latencies[int(n*0.5)]:.1f} ms')
    print(f'P90: {latencies[int(n*0.9)]:.1f} ms')
    print(f'P95: {latencies[int(n*0.95)]:.1f} ms')
    print(f'P99: {latencies[int(n*0.99)]:.1f} ms')
    print(f'Max: {max(latencies):.1f} ms')
    print(f'Min: {min(latencies):.1f} ms')
