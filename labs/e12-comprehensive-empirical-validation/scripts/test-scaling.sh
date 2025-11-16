#!/bin/bash

# Test scaling behavior of traditional baseline
# Runs 2, 12, 24, 48, 96 node tests @ 1Gbps to measure growth pattern

set -e

cd "$(dirname "$0")"

# Load environment
if [ -f ../../../.env ]; then
    set -a
    source ../../../.env
    set +a
fi

RESULTS_DIR="scaling-test-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$RESULTS_DIR"

TEST_DURATION=60

log_info() {
    echo "→ $1"
}

log_success() {
    echo "✓ $1"
}

run_scaling_test() {
    local scale=$1
    local topology=$2
    local test_name="traditional-${scale}node-1gbps"
    local test_dir="$RESULTS_DIR/$test_name"

    echo ""
    echo "========================================"
    echo "Scaling Test: $scale nodes"
    echo "========================================"

    mkdir -p "$test_dir/docker-stats"
    mkdir -p "$test_dir/logs"

    # Deploy
    log_info "Deploying $scale node topology..."
    containerlab deploy -t "$topology" --reconfigure > /dev/null 2>&1
    sleep 5
    log_success "Deployed"

    # Start stats collection
    log_info "Starting stats collection..."
    (
        while true; do
            timestamp=$(date +%s)
            containers=$(docker ps --filter "name=clab-" --format "{{.Names}}")
            if [ -n "$containers" ]; then
                docker stats --no-stream --format "json" $containers > "$test_dir/docker-stats/stats-$timestamp.json" 2>/dev/null || true
            fi
            sleep 5
        done
    ) &
    STATS_PID=$!
    log_success "Stats collection started (PID: $STATS_PID)"

    # Run test
    log_info "Running ${TEST_DURATION}s test..."
    sleep $TEST_DURATION

    # Stop stats
    log_info "Stopping stats collection..."
    kill $STATS_PID 2>/dev/null || true
    log_success "Stats stopped"

    # Collect logs
    log_info "Collecting logs from $scale containers..."
    local containers=$(docker ps --filter "name=clab-" --format "{{.Names}}")
    local count=0
    for container in $containers; do
        timeout 30 docker logs "$container" > "$test_dir/logs/${container}.log" 2>&1 || true
        count=$((count + 1))
    done
    log_success "Collected $count logs"

    # Cleanup
    log_info "Cleaning up containers..."
    docker ps -a --filter "name=clab-" --format "{{.Names}}" | xargs -r docker rm -f > /dev/null 2>&1 || true
    sleep 1
    log_success "Test complete"
}

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Traditional Baseline Scaling Test                        ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Testing scales: 2, 12, 24, 48, 96 nodes @ 1Gbps"
echo "Results directory: $RESULTS_DIR"
echo ""

# Run tests
run_scaling_test "2" "../../../hive-sim/topologies/traditional-2node.yaml"
run_scaling_test "12" "../../../hive-sim/topologies/squad-12node-client-server.yaml"
run_scaling_test "24" "../../../hive-sim/topologies/traditional-platoon-24node.yaml"
run_scaling_test "48" "../../../hive-sim/topologies/traditional-battalion-48node.yaml"
run_scaling_test "96" "../../../hive-sim/topologies/traditional-battalion-96node.yaml"

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Scaling Tests Complete - Processing Results              ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Post-process all results
for test_dir in "$RESULTS_DIR"/traditional-*; do
    echo "Processing: $(basename $test_dir)"
    ./post-process-tests.sh "$test_dir" 2>/dev/null
done

# Generate scaling analysis
python3 << 'PYTHON_EOF'
import json
from pathlib import Path
import sys

results_dir = Path(sys.argv[1])

print("\n" + "=" * 80)
print("EMPIRICAL SCALING RESULTS")
print("=" * 80)
print()

scales = []
for test_dir in sorted(results_dir.iterdir()):
    if not test_dir.is_dir():
        continue

    parts = test_dir.name.split('-')
    scale = int(parts[1].replace("node", ""))

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
            "per_node_kb": (total_bytes / node_count) / 1e3 if node_count > 0 else 0
        })

print(f"{'Nodes':>6} {'Total Traffic':>15} {'Per-Node':>15} {'Growth Factor':>15} {'Scaling':>15}")
print("-" * 80)

prev = None
for s in scales:
    growth = ""
    scaling = ""
    if prev:
        growth_factor = s["total_mb"] / prev["total_mb"]
        node_factor = s["nodes"] / prev["nodes"]

        # Expected growth for different complexities
        if abs(growth_factor - node_factor) < 0.3 * node_factor:
            scaling = "O(n) Linear"
        elif abs(growth_factor - (node_factor ** 2)) < 0.3 * (node_factor ** 2):
            scaling = "O(n²) Quadratic"
        else:
            scaling = f"~O(n^{(growth_factor / node_factor):.1f})"

        growth = f"{growth_factor:.2f}x"

    print(f"{s['nodes']:>6} {s['total_mb']:>13.2f} MB {s['per_node_kb']:>13.1f} KB {growth:>15} {scaling:>15}")
    prev = s

print()
print("Analysis:")
if len(scales) >= 2:
    first = scales[0]
    last = scales[-1]
    node_factor = last["nodes"] / first["nodes"]
    traffic_factor = last["total_mb"] / first["total_mb"]

    print(f"  Node increase: {first['nodes']} → {last['nodes']} = {node_factor:.1f}x")
    print(f"  Traffic increase: {first['total_mb']:.2f}MB → {last['total_mb']:.2f}MB = {traffic_factor:.1f}x")
    print()

    if traffic_factor < node_factor * 1.5:
        print("  ✓ Scaling appears LINEAR O(n)")
    elif traffic_factor > node_factor ** 1.5:
        print("  ⚠ Scaling appears SUPER-LINEAR (worse than O(n))")
        print(f"  Estimated complexity: O(n^{(traffic_factor / node_factor):.2f})")
    else:
        print(f"  ~ Scaling between linear and quadratic")

PYTHON_EOF

python3 -c "import sys; sys.argv.append('$RESULTS_DIR')" "$RESULTS_DIR"

echo ""
echo "Results saved to: $RESULTS_DIR"
