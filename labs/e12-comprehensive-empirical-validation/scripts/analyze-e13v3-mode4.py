#!/usr/bin/env python3
import json
import glob
import statistics

latencies = []
for logfile in glob.glob('e13v3-mode4-hierarchical-20251113-205349/mode4-96node-1gbps/*.log'):
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
    print(f'=== Mode 4 Hierarchical - InitialPropagation Latencies (96 nodes @ 1Gbps) ===')
    print(f'Total events: {n}')
    print(f'Mean: {statistics.mean(latencies):.1f} ms')
    print(f'Median (P50): {latencies[int(n*0.5)]:.1f} ms')
    print(f'P90: {latencies[int(n*0.9)]:.1f} ms')
    print(f'P95: {latencies[int(n*0.95)]:.1f} ms')
    print(f'P99: {latencies[int(n*0.99)]:.1f} ms')
    print(f'Max: {max(latencies):.1f} ms')
    print(f'Min: {min(latencies):.1f} ms')
else:
    print("No InitialPropagation events found!")
