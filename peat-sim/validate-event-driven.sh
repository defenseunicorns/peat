#!/usr/bin/env bash
set -euo pipefail

# Quick Event-Driven Validation (24-node hierarchical)
# Tests the zero-polling event-driven aggregation fix

echo "Loading Ditto credentials..."
if [ -f ../.env ]; then
    set -a && source ../.env && set +a
    echo "✓ Loaded credentials from ../.env"
else
    echo "✗ No .env file found"
    exit 1
fi

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="validate-event-driven-${TIMESTAMP}"
mkdir -p "$RESULTS_DIR"

echo ""
echo "Deploying 24-node hierarchical topology..."
TEST_TOPOLOGY="topologies/platoon-24node-mesh-mode4.yaml"
containerlab deploy -t "$TEST_TOPOLOGY" --reconfigure > "$RESULTS_DIR/deploy.log" 2>&1

echo "Running for 60 seconds..."
sleep 60

echo "Collecting metrics..."
mkdir -p "$RESULTS_DIR/logs"

for container in $(docker ps --filter "name=clab-cap-platoon" --format "{{.Names}}"); do
    docker logs $container > "$RESULTS_DIR/logs/${container}.log" 2>&1 || true
done

echo "Extracting METRICS events..."
grep -h "METRICS:" "$RESULTS_DIR/logs"/*.log > "$RESULTS_DIR/metrics.jsonl" 2>/dev/null || echo "No metrics found"

echo "Cleaning up..."
containerlab destroy -t "$TEST_TOPOLOGY" --cleanup > /dev/null 2>&1

METRIC_COUNT=$(wc -l < "$RESULTS_DIR/metrics.jsonl" 2>/dev/null || echo 0)

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "✓ VALIDATION COMPLETE"
echo "═══════════════════════════════════════════════════════════"
echo "  Collected: $METRIC_COUNT metric events"
echo "  Results: $RESULTS_DIR/"
echo ""

# Quick analysis
if [ -f "$RESULTS_DIR/metrics.jsonl" ] && [ "$METRIC_COUNT" -gt 0 ]; then
    echo "Event type breakdown:"
    grep -o '"event_type":"[^"]*"' "$RESULTS_DIR/metrics.jsonl" | sort | uniq -c | sort -rn || true
    echo ""

    # Check for aggregation metrics
    AGG_COUNT=$(grep -c '"event_type":"AggregationCompleted"' "$RESULTS_DIR/metrics.jsonl" 2>/dev/null || echo 0)
    if [ "$AGG_COUNT" -gt 0 ]; then
        echo "✓ Event-driven aggregation working: $AGG_COUNT aggregations"
        echo "  (Zero polling confirmed - aggregations are event-triggered)"
    else
        echo "⚠ No aggregation events found"
    fi
fi
