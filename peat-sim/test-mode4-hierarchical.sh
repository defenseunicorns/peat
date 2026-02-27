#!/bin/bash
# Test Mode 4 (Hierarchical Aggregation) - E11 Validation
#
# This script validates hierarchical aggregation bandwidth reduction:
# - Expected: O(n log n) scaling vs O(n²) in baseline
# - 24-node platoon = 3 squads of ~7 members each
# - Target: 27 total ops (24 NodeStates + 3 SquadSummaries) vs 576 in Mode 2

set -e

cd "$(dirname "$0")"

echo "======================================"
echo "Mode 4: Hierarchical Aggregation Test"
echo "======================================"
echo ""

# Load environment variables
set -a
source ../.env 2>/dev/null || true
set +a

# Test configuration
TOPOLOGY="topologies/platoon-24node-mesh-mode4.yaml"
TEST_DURATION=60  # seconds
RESULTS_DIR="test-results-mode4-$(date +%Y%m%d-%H%M%S)"

mkdir -p "$RESULTS_DIR"

echo "Topology: $TOPOLOGY"
echo "Test Duration: ${TEST_DURATION}s"
echo "Results Dir: $RESULTS_DIR"
echo ""

# Check if already deployed
if docker ps | grep -q "clab-cap-platoon-mode4"; then
    echo "✓ Mode 4 topology already running"
else
    echo "Deploying Mode 4 topology..."
    containerlab deploy --topo "$TOPOLOGY"
    echo "Waiting 30s for network stabilization..."
    sleep 30
fi

echo ""
echo "======================================"
echo "Running test for ${TEST_DURATION}s..."
echo "======================================"
echo ""

# Collect initial state
echo "[T=0] Collecting initial state..."
docker logs clab-cap-platoon-mode4-client-server-squad-alpha-leader 2>&1 | tail -20 > "$RESULTS_DIR/squad-alpha-initial.log"
docker logs clab-cap-platoon-mode4-client-server-squad-bravo-leader 2>&1 | tail -20 > "$RESULTS_DIR/squad-bravo-initial.log"
docker logs clab-cap-platoon-mode4-client-server-squad-charlie-leader 2>&1 | tail -20 > "$RESULTS_DIR/squad-charlie-initial.log"
docker logs clab-cap-platoon-mode4-client-server-platoon-leader 2>&1 | tail -20 > "$RESULTS_DIR/platoon-leader-initial.log"

# Wait for test duration
for i in $(seq 1 $TEST_DURATION); do
    if [ $((i % 15)) -eq 0 ]; then
        echo "[T=${i}s] Test running..."
    fi
    sleep 1
done

echo ""
echo "======================================"
echo "Test complete. Collecting results..."
echo "======================================"
echo ""

# Collect final logs
echo "Collecting logs from all nodes..."
for container in $(docker ps --format '{{.Names}}' | grep "clab-cap-platoon-mode4"); do
    node_name=$(echo "$container" | sed 's/clab-cap-platoon-mode4-client-server-//')
    docker logs "$container" 2>&1 > "$RESULTS_DIR/${node_name}.log"
done

echo "✓ Logs collected"
echo ""

# Analyze results
echo "======================================"
echo "Analysis"
echo "======================================"
echo ""

# Count squad aggregations
echo "Squad Aggregation Messages:"
for squad in alpha bravo charlie; do
    count=$(grep -c "Aggregated squad" "$RESULTS_DIR/squad-${squad}-leader.log" || echo "0")
    members=$(grep "Aggregated squad" "$RESULTS_DIR/squad-${squad}-leader.log" | tail -1 | grep -oP '\d+ members' || echo "? members")
    echo "  Squad $squad: $count aggregations ($members)"
done
echo ""

# Count platoon aggregations
echo "Platoon Aggregation Messages:"
platoon_count=$(grep -c "Aggregated platoon" "$RESULTS_DIR/platoon-leader.log" || echo "0")
echo "  Platoon-1: $platoon_count aggregations"
echo ""

# Extract METRICS from all logs
echo "Extracting metrics..."
grep "METRICS:" "$RESULTS_DIR"/*.log > "$RESULTS_DIR/all-metrics.jsonl" || true

# Count message types
message_sent_count=$(grep -c '"event_type":"MessageSent"' "$RESULTS_DIR/all-metrics.jsonl" || echo "0")
doc_inserted_count=$(grep -c '"event_type":"DocumentInserted"' "$RESULTS_DIR/all-metrics.jsonl" || echo "0")

echo "Total MessageSent events: $message_sent_count"
echo "Total DocumentInserted events: $doc_inserted_count"
echo ""

# Calculate theoretical vs actual
NODES=24
SQUADS=3
THEORETICAL_BASELINE=$((NODES * NODES))  # O(n²)
THEORETICAL_MODE4=$((NODES + SQUADS))     # O(n log n) approximation

echo "Theoretical Comparison (per update cycle):"
echo "  Baseline (Mode 2): $THEORETICAL_BASELINE operations (O(n²))"
echo "  Mode 4 (Hierarchical): $THEORETICAL_MODE4 operations (O(n log n))"
echo "  Reduction: $(awk "BEGIN {printf \"%.1f\", (1 - $THEORETICAL_MODE4/$THEORETICAL_BASELINE) * 100}")%"
echo ""

echo "======================================"
echo "Test Results Summary"
echo "======================================"
echo ""
echo "Results saved to: $RESULTS_DIR/"
echo ""
echo "Key files:"
echo "  - all-metrics.jsonl: All METRICS events"
echo "  - squad-*-leader.log: Squad aggregation logs"
echo "  - platoon-leader.log: Platoon aggregation logs"
echo ""
echo "To view squad aggregations:"
echo "  grep 'Aggregated squad' $RESULTS_DIR/squad-*-leader.log"
echo ""
echo "To view platoon aggregations:"
echo "  grep 'Aggregated platoon' $RESULTS_DIR/platoon-leader.log"
echo ""
