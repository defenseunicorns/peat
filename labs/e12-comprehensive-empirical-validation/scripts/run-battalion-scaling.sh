#!/bin/bash
set -e

cd "$(dirname "$0")"

if [ -f ../../../.env ]; then
    set -a
    source ../../../.env
    set +a
fi

RESULTS_DIR="e12-comprehensive-results-20251110-115542"

run_test() {
    local nodes=$1
    local topology=$2
    local test_name="traditional-${nodes}node-1gbps"
    local test_dir="$RESULTS_DIR/$test_name"

    echo ""
    echo "========================================"
    echo "Battalion Scale Test: $nodes nodes"
    echo "========================================"

    rm -rf "$test_dir"
    mkdir -p "$test_dir/docker-stats" "$test_dir/logs"

    echo "→ Deploying..."
    containerlab deploy -t "$topology" --reconfigure > /dev/null 2>&1 && echo "✓ Deployed" && sleep 5

    echo "→ Starting stats..."
    (while true; do
        containers=$(docker ps --filter "name=clab-" --format "{{.Names}}")
        [ -n "$containers" ] && docker stats --no-stream --format "json" $containers > "$test_dir/docker-stats/stats-$(date +%s).json" 2>/dev/null
        sleep 5
    done) &
    STATS_PID=$!
    echo "✓ Stats started (PID: $STATS_PID)"

    echo "→ Running 60s test..."
    sleep 60

    echo "→ Stopping stats..."
    kill $STATS_PID 2>/dev/null && echo "✓ Stopped"

    echo "→ Collecting logs..."
    for c in $(docker ps --filter "name=clab-" --format "{{.Names}}"); do
        timeout 30 docker logs "$c" > "$test_dir/logs/$c.log" 2>&1 &
    done
    wait
    echo "✓ Collected"

    echo "→ Cleanup..."
    docker ps -a --filter "name=clab-" --format "{{.Names}}" | xargs -r docker rm -f > /dev/null 2>&1
    echo "✓ Complete"
}

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Battalion Scaling Tests (48 & 96 nodes)                  ║"
echo "╚════════════════════════════════════════════════════════════╝"

run_test 48 "../../../cap-sim/topologies/traditional-battalion-48node.yaml"
run_test 96 "../../../cap-sim/topologies/traditional-battalion-96node.yaml"

echo ""
echo "Post-processing..."
./post-process-tests.sh "$RESULTS_DIR"/traditional-48node-* "$RESULTS_DIR"/traditional-96node-* 2>/dev/null
echo "✓ Done"
