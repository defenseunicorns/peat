#!/usr/bin/env python3
import json
from pathlib import Path
import sys

results_dir = Path("e12-comprehensive-results-20251110-115542")

print("=" * 80)
print("TRADITIONAL IoT BASELINE - EMPIRICAL SCALING ANALYSIS")
print("(All tests @ 1Gbps, 2Hz updates, 60 second duration)")
print("=" * 80)
print()

# Collect traditional baseline data at 1Gbps
scales = []
for test_dir in sorted(results_dir.iterdir()):
    if not test_dir.is_dir() or not test_dir.name.startswith("traditional-"):
        continue

    # Only look at 1gbps tests for scaling analysis
    if "1gbps" not in test_dir.name:
        continue

    docker_stats = test_dir / "docker-stats-summary.json"
    if not docker_stats.exists():
        continue

    with open(docker_stats) as f:
        stats = json.load(f)
        total_bytes = sum(node.get("net_total_bytes", 0) for node in stats.values())
        node_count = len(stats)

        scales.append({
            "nodes": node_count,
            "total_mb": total_bytes / 1e6,
            "per_node_kb": (total_bytes / node_count) / 1e3 if node_count > 0 else 0,
            "test_name": test_dir.name
        })

# Sort by node count
scales.sort(key=lambda x: x["nodes"])

# Print table
print(f"{'Nodes':>6} {'Total Traffic':>15} {'Per-Node':>15} {'Growth':>12} {'Complexity':>15}")
print("-" * 80)

prev = None
for s in scales:
    growth = ""
    complexity = ""

    if prev:
        growth_factor = s["total_mb"] / prev["total_mb"] if prev["total_mb"] > 0 else 0
        node_factor = s["nodes"] / prev["nodes"]

        # Calculate empirical exponent
        if node_factor > 1 and growth_factor > 0:
            import math
            empirical_exp = math.log(growth_factor) / math.log(node_factor)

            if abs(empirical_exp - 1.0) < 0.3:
                complexity = "O(n) ✓"
            elif abs(empirical_exp - 2.0) < 0.3:
                complexity = "O(n²) ⚠"
            else:
                complexity = f"O(n^{empirical_exp:.1f}) ⚠"

        growth = f"{growth_factor:.2f}x"

    print(f"{s['nodes']:>6} {s['total_mb']:>13.2f} MB {s['per_node_kb']:>13.1f} KB {growth:>12} {complexity:>15}")
    prev = s

print()
print("=" * 80)
print("OVERALL SCALING BEHAVIOR")
print("=" * 80)

if len(scales) >= 2:
    import math
    first = scales[0]
    last = scales[-1]

    node_increase = last["nodes"] / first["nodes"]
    traffic_increase = last["total_mb"] / first["total_mb"]

    # Calculate overall complexity
    overall_exp = math.log(traffic_increase) / math.log(node_increase)

    print()
    print(f"Node count:      {first['nodes']:>6} → {last['nodes']:>6} nodes  ({node_increase:.1f}x increase)")
    print(f"Total traffic:   {first['total_mb']:>6.2f} → {last['total_mb']:>6.2f} MB     ({traffic_increase:.1f}x increase)")
    print()
    print(f"Empirical complexity: O(n^{overall_exp:.2f})")
    print()

    if overall_exp < 1.2:
        print("✓ Scaling is LINEAR - excellent")
    elif overall_exp < 1.5:
        print("⚠ Scaling is SUPER-LINEAR - approaching quadratic")
    elif overall_exp < 2.2:
        print("⚠ Scaling is approaching QUADRATIC - poor scalability")
    else:
        print("✗ Scaling is WORSE THAN QUADRATIC - severe scalability issues")

    print()
    print("=" * 80)
    print("DIVISION-SCALE PROJECTIONS")
    print("=" * 80)
    print()

    # Project to larger scales
    base_nodes = last["nodes"]
    base_traffic = last["total_mb"]

    print(f"{'Scale':>10} {'Nodes':>10} {'Projected Traffic':>20}")
    print("-" * 50)

    for scale_name, target_nodes in [
        ("Battalion", 192),
        ("Battalion", 384),
        ("Division", 768),
        ("Division", 1536)
    ]:
        factor = target_nodes / base_nodes
        projected = base_traffic * (factor ** overall_exp)
        print(f"{scale_name:>10} {target_nodes:>10} {projected:>18.2f} MB")

    print()
    print(f"Note: Projections based on measured O(n^{overall_exp:.2f}) complexity")
    print()
