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

# Check for soldier CRDT metrics
SOLDIER_COUNT=$(grep -h 'METRICS:.*"tier":"soldier".*CRDTUpsert' lab4-quick-validation/soldier.log 2>/dev/null | wc -l)
echo "  ✓ Soldier CRDT operations: $SOLDIER_COUNT"

# Check for squad leader CRDT metrics
SQUAD_COUNT=$(grep -h 'METRICS:.*"tier":"squad_leader".*CRDTUpsert' lab4-quick-validation/*squad*leader*.log 2>/dev/null | wc -l)
echo "  ✓ Squad leader CRDT operations: $SQUAD_COUNT"

# Check for platoon leader CRDT metrics
PLATOON_COUNT=$(grep -h 'METRICS:.*"tier":"platoon_leader".*CRDTUpsert' lab4-quick-validation/platoon-leader.log 2>/dev/null | wc -l)
echo "  ✓ Platoon leader CRDT operations: $PLATOON_COUNT"

# Check for aggregation efficiency metrics
AGG_COUNT=$(grep -h 'METRICS:.*AggregationEfficiency' lab4-quick-validation/*leader*.log 2>/dev/null | wc -l)
echo "  ✓ Aggregation efficiency events: $AGG_COUNT"

# Calculate sample latencies
if [ -f "lab4-quick-validation/soldier.log" ]; then
    grep 'METRICS:' lab4-quick-validation/soldier.log | \
        grep '"tier":"soldier"' | \
        grep '"event_type":"CRDTUpsert"' | \
        sed 's/.*"latency_ms":\([0-9.]*\).*/\1/' | \
        sort -n > /tmp/soldier_lat.txt || true

    if [ -s /tmp/soldier_lat.txt ]; then
        SOLDIER_P50=$(awk 'BEGIN{c=0} {a[c++]=$1} END{print a[int(c*0.5)]}' /tmp/soldier_lat.txt)
        SOLDIER_P95=$(awk 'BEGIN{c=0} {a[c++]=$1} END{print a[int(c*0.95)]}' /tmp/soldier_lat.txt)
        echo "  ✓ Soldier latency: P50=${SOLDIER_P50}ms, P95=${SOLDIER_P95}ms"
    fi
    rm -f /tmp/soldier_lat.txt
fi

echo ""
echo "Cleanup..."
containerlab destroy -t "$TOPO_FILE" --cleanup

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Lab 4 Quick Validation Complete                          ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

if [ "$SOLDIER_COUNT" -gt 0 ] && [ "$SQUAD_COUNT" -gt 0 ]; then
    echo "✅ SUCCESS: Lab 4 metrics instrumentation is working"
    echo ""
    echo "Metrics collected:"
    echo "  - Soldier CRDT latencies: $SOLDIER_COUNT samples"
    echo "  - Squad leader CRDT latencies: $SQUAD_COUNT samples"
    echo "  - Platoon leader CRDT latencies: $PLATOON_COUNT samples"
    echo "  - Aggregation efficiency events: $AGG_COUNT events"
    echo ""
    echo "Ready to run full Lab 4 test suite:"
    echo "  ./test-lab4-hierarchical-hive-crdt.sh"
else
    echo "⚠️  WARNING: Limited metrics collected"
    echo "  Soldier metrics: $SOLDIER_COUNT"
    echo "  Squad metrics: $SQUAD_COUNT"
    echo ""
    echo "Check logs in lab4-quick-validation/ for debugging"
fi
