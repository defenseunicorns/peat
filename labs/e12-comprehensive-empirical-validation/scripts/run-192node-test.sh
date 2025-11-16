#!/bin/bash
set -e

RESULTS_DIR="e12-comprehensive-results-20251110-115542"
TEST_NAME="traditional-192node-1gbps"
TEST_DIR="$RESULTS_DIR/$TEST_NAME"
TOPOLOGY="../../../hive-sim/topologies/traditional-battalion-192node.yaml"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Battalion Scale Test: 192 nodes (8 platoons)            ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR/docker-stats" "$TEST_DIR/logs"

echo "→ Deploying 192-node topology..."
containerlab deploy -t "$TOPOLOGY" --reconfigure > /dev/null 2>&1
echo "✓ Deployed"
sleep 5

echo "→ Starting stats collection..."
(while true; do
    containers=$(docker ps --filter "name=clab-" --format "{{.Names}}")
    [ -n "$containers" ] && docker stats --no-stream --format "json" $containers > "$TEST_DIR/docker-stats/stats-$(date +%s).json" 2>/dev/null
    sleep 5
done) &
STATS_PID=$!
echo "✓ Stats started (PID: $STATS_PID)"

echo "→ Running 60s test..."
sleep 60

echo "→ Stopping stats..."
kill $STATS_PID 2>/dev/null
echo "✓ Stopped"

echo "→ Collecting logs from 193 containers..."
for c in $(docker ps --filter "name=clab-" --format "{{.Names}}"); do
    timeout 30 docker logs "$c" > "$TEST_DIR/logs/$c.log" 2>&1 &
done
wait
echo "✓ Collected"

echo "→ Cleanup..."
docker ps -a --filter "name=clab-" --format "{{.Names}}" | xargs -r docker rm -f > /dev/null 2>&1
echo "✓ Complete"

echo ""
echo "Post-processing..."
./post-process-tests.sh "$TEST_DIR" 2>/dev/null
echo "✓ Done"

echo ""
echo "Results saved to: $TEST_DIR"
