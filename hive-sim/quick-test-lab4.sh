#!/bin/bash
# Quick Lab 4 Validation Test
#
# Runs a single 24-node hierarchical test to validate Lab 4 infrastructure

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

source ./test-common.sh

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Lab 4 Quick Validation (24 nodes, 1 Gbps)               ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

validate_environment

TEST_NAME="lab4-quick-test-24n-1gbps"
TOPO_FILE="topologies/test-backend-ditto-24n-hierarchical-1gbps.yaml"

if [ ! -f "$TOPO_FILE" ]; then
    echo "❌ Topology not found: $TOPO_FILE"
    echo "   Run from hive-sim directory"
    exit 1
fi

echo "Deploying 24-node hierarchical topology..."
containerlab deploy -t "$TOPO_FILE" --reconfigure

echo ""
echo "Running for 2 minutes to collect metrics..."
sleep 120

echo ""
echo "Collecting logs and metrics..."
mkdir -p lab4-quick-validation

LAB_NAME="cap-platoon-mode4-mesh"

# Collect soldier log
SOLDIER=$(docker ps --format '{{.Names}}' | grep "${LAB_NAME}" | grep "soldier" | head -1)
if [ -n "$SOLDIER" ]; then
    echo "  Soldier: $SOLDIER"
    docker logs "$SOLDIER" 2>&1 > lab4-quick-validation/soldier.log
fi

# Collect squad leader logs
docker ps --format '{{.Names}}' | grep "${LAB_NAME}" | grep "squad.*leader" | while read CONTAINER; do
    echo "  Squad Leader: $CONTAINER"
    docker logs "$CONTAINER" 2>&1 > "lab4-quick-validation/${CONTAINER}.log"
done

# Collect platoon leader log
PLATOON=$(docker ps --format '{{.Names}}' | grep "${LAB_NAME}" | grep "platoon-leader" | head -1)
if [ -n "$PLATOON" ]; then
    echo "  Platoon Leader: $PLATOON"
    docker logs "$PLATOON" 2>&1 > lab4-quick-validation/platoon-leader.log
fi

echo ""
echo "Analyzing metrics..."

# Check for soldier metrics (MessageSent events - less critical)
SOLDIER_COUNT=$(grep -h 'METRICS:.*MessageSent' lab4-quick-validation/soldier.log 2>/dev/null | wc -l)
echo "  ✓ Soldier message events: $SOLDIER_COUNT"

# Check for squad leader aggregation metrics
SQUAD_COUNT=$(grep -h 'METRICS:.*"event_type":"AggregationCompleted".*"tier":"squad"' lab4-quick-validation/*squad*leader*.log 2>/dev/null | wc -l)
echo "  ✓ Squad leader aggregation operations: $SQUAD_COUNT"

# Check for platoon leader aggregation metrics
PLATOON_COUNT=$(grep -h 'METRICS:.*"event_type":"AggregationCompleted".*"tier":"platoon"' lab4-quick-validation/platoon-leader.log 2>/dev/null | wc -l)
echo "  ✓ Platoon leader aggregation operations: $PLATOON_COUNT"

# Calculate squad leader aggregation latencies
grep -h 'METRICS:' lab4-quick-validation/*squad*leader*.log 2>/dev/null | \
    grep '"event_type":"AggregationCompleted"' | \
    grep '"tier":"squad"' | \
    sed 's/.*"processing_time_us":\([0-9.]*\).*/\1/' | \
    awk '{print $1/1000}' | \
    sort -n > /tmp/squad_lat.txt || true

if [ -s /tmp/squad_lat.txt ]; then
    SQUAD_P50=$(awk 'BEGIN{c=0} {a[c++]=$1} END{print a[int(c*0.5)]}' /tmp/squad_lat.txt)
    SQUAD_P95=$(awk 'BEGIN{c=0} {a[c++]=$1} END{print a[int(c*0.95)]}' /tmp/squad_lat.txt)
    echo "  ✓ Squad aggregation latency: P50=${SQUAD_P50}ms, P95=${SQUAD_P95}ms"
fi
rm -f /tmp/squad_lat.txt

# Calculate aggregation ratio
AGG_RATIO=$(grep -h 'METRICS:' lab4-quick-validation/*squad*leader*.log 2>/dev/null | \
    grep '"event_type":"AggregationCompleted"' | \
    grep '"tier":"squad"' | \
    sed 's/.*"input_count":\([0-9]*\).*/\1/' | \
    head -1 || echo "0")
echo "  ✓ Aggregation ratio: ${AGG_RATIO}:1 reduction"

echo ""
echo "Cleanup..."
containerlab destroy -t "$TOPO_FILE" --cleanup

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Lab 4 Quick Validation Complete                          ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

if [ "$SQUAD_COUNT" -gt 0 ]; then
    echo "✅ SUCCESS: Lab 4 hierarchical aggregation metrics are working"
    echo ""
    echo "Metrics collected:"
    echo "  - Soldier message events: $SOLDIER_COUNT"
    echo "  - Squad aggregation operations: $SQUAD_COUNT"
    echo "  - Platoon aggregation operations: $PLATOON_COUNT"
    echo "  - Aggregation ratio: ${AGG_RATIO}:1"
    echo ""
    echo "Ready to run full Lab 4 test suite:"
    echo "  ./test-lab4-hierarchical-hive-crdt.sh"
else
    echo "⚠️  WARNING: Limited metrics collected"
    echo "  Squad aggregation metrics: $SQUAD_COUNT"
    echo ""
    echo "Check logs in lab4-quick-validation/ for debugging"
fi
