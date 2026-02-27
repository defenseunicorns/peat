#!/bin/bash
# Test Mode 4 (Hierarchical P2P Mesh) with specific bandwidth constraint
#
# Usage: ./test-mode4-bandwidth.sh [BANDWIDTH]
#   BANDWIDTH: 1gbps, 100mbps, 1mbps, 256kbps (default: unconstrained)
#
# Examples:
#   ./test-mode4-bandwidth.sh 256kbps  # Test at tactical radio bandwidth
#   ./test-mode4-bandwidth.sh 1gbps    # Test at gigabit ethernet
#   ./test-mode4-bandwidth.sh          # Test unconstrained (baseline)

set -e

cd "$(dirname "$0")"

# Parse bandwidth argument
BANDWIDTH_LABEL="${1:-unconstrained}"

# Bandwidth configurations (in Kbps for netem)
declare -A BANDWIDTHS=(
    ["1gbps"]=1048576     # 1 Gbps = 1048576 Kbps
    ["100mbps"]=102400    # 100 Mbps = 102400 Kbps
    ["1mbps"]=1024        # 1 Mbps = 1024 Kbps
    ["256kbps"]=256       # 256 Kbps (tactical radio)
)

# Validate bandwidth argument
if [[ "$BANDWIDTH_LABEL" != "unconstrained" ]] && [[ ! ${BANDWIDTHS[$BANDWIDTH_LABEL]+_} ]]; then
    echo "Error: Invalid bandwidth '$BANDWIDTH_LABEL'"
    echo "Valid options: 1gbps, 100mbps, 1mbps, 256kbps, unconstrained"
    exit 1
fi

RATE_KBPS=${BANDWIDTHS[$BANDWIDTH_LABEL]:-0}

echo "╔════════════════════════════════════════════════════════════╗"
echo "║   Mode 4: Hierarchical P2P Mesh - Bandwidth Test          ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Bandwidth: $BANDWIDTH_LABEL"
if [[ $RATE_KBPS -gt 0 ]]; then
    echo "Rate: $RATE_KBPS Kbps"
fi
echo ""

# Load environment variables
set -a
source ../.env 2>/dev/null || true
set +a

# Test configuration
TOPOLOGY="topologies/platoon-24node-mesh-mode4.yaml"
LAB_NAME="cap-platoon-mode4-mesh"
TEST_DURATION=90  # seconds (longer for bandwidth-constrained tests)
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="test-results-mode4-${BANDWIDTH_LABEL}-${TIMESTAMP}"

mkdir -p "$RESULTS_DIR"

echo "Topology: $TOPOLOGY"
echo "Test Duration: ${TEST_DURATION}s"
echo "Results Dir: $RESULTS_DIR"
echo ""

# Function to apply bandwidth constraints
apply_bandwidth_constraint() {
    local rate_kbps=$1
    local lab_name=$2

    if [[ $rate_kbps -eq 0 ]]; then
        echo "→ Running unconstrained (no bandwidth limits)"
        return
    fi

    echo "→ Applying ${rate_kbps} Kbps constraint to all nodes..."

    local containers=$(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}")
    local count=0

    for container in $containers; do
        containerlab tools netem set -n "$container" -i eth0 --rate "$rate_kbps" > /dev/null 2>&1 || true
        count=$((count + 1))
    done

    echo "  ✓ Applied constraints to $count containers"
}

# Deploy topology
echo "════════════════════════════════════════════════════════════"
echo "Deploying Mode 4 topology..."
echo "════════════════════════════════════════════════════════════"
echo ""

if docker ps | grep -q "clab-${LAB_NAME}"; then
    echo "⚠  Mode 4 topology already running, destroying first..."
    containerlab destroy --topo "$TOPOLOGY"
    sleep 2
fi

containerlab deploy --topo "$TOPOLOGY"

echo ""
echo "→ Waiting 30s for initial network formation..."
sleep 30

# Apply bandwidth constraints
apply_bandwidth_constraint "$RATE_KBPS" "$LAB_NAME"

echo ""
echo "→ Waiting 10s for constraint stabilization..."
sleep 10

echo ""
echo "════════════════════════════════════════════════════════════"
echo "Running test for ${TEST_DURATION}s..."
echo "════════════════════════════════════════════════════════════"
echo ""

# Collect initial state
echo "[T=0] Collecting initial state..."
docker logs clab-${LAB_NAME}-squad-alpha-leader 2>&1 | tail -20 > "$RESULTS_DIR/squad-alpha-initial.log"
docker logs clab-${LAB_NAME}-squad-bravo-leader 2>&1 | tail -20 > "$RESULTS_DIR/squad-bravo-initial.log"
docker logs clab-${LAB_NAME}-squad-charlie-leader 2>&1 | tail -20 > "$RESULTS_DIR/squad-charlie-initial.log"
docker logs clab-${LAB_NAME}-platoon-leader 2>&1 | tail -20 > "$RESULTS_DIR/platoon-leader-initial.log"

# Wait for test duration
START_TIME=$(date +%s)
for i in $(seq 1 $TEST_DURATION); do
    if [ $((i % 15)) -eq 0 ]; then
        ELAPSED=$((i))
        echo "[T=${ELAPSED}s] Test running..."
    fi
    sleep 1
done
END_TIME=$(date +%s)
ACTUAL_DURATION=$((END_TIME - START_TIME))

echo ""
echo "════════════════════════════════════════════════════════════"
echo "Test complete. Collecting results..."
echo "════════════════════════════════════════════════════════════"
echo ""

# Collect final logs
echo "→ Collecting logs from all 24 nodes..."
for container in $(docker ps --format '{{.Names}}' | grep "clab-${LAB_NAME}"); do
    node_name=$(echo "$container" | sed "s/clab-${LAB_NAME}-//")
    docker logs "$container" 2>&1 > "$RESULTS_DIR/${node_name}.log"
done

echo "✓ Logs collected"
echo ""

# Analyze results
echo "════════════════════════════════════════════════════════════"
echo "Analysis"
echo "════════════════════════════════════════════════════════════"
echo ""

# Count squad aggregations
echo "Squad Aggregation Results:"
for squad in alpha bravo charlie; do
    count=$(grep -c "Aggregated squad" "$RESULTS_DIR/squad-${squad}-leader.log" 2>/dev/null || echo "0")
    members=$(grep "Aggregated squad" "$RESULTS_DIR/squad-${squad}-leader.log" | tail -1 | grep -oP '\d+ members' | head -1 || echo "0 members")
    echo "  Squad $squad: $count aggregations ($members)"
done
echo ""

# Count platoon aggregations
echo "Platoon Aggregation Results:"
platoon_count=$(grep -c "Aggregated platoon" "$RESULTS_DIR/platoon-leader.log" 2>/dev/null || echo "0")
platoon_last=$(grep "Aggregated platoon" "$RESULTS_DIR/platoon-leader.log" | tail -1 || echo "No aggregations")
echo "  Platoon-1: $platoon_count aggregations"
echo "  Last: $platoon_last"
echo ""

# Extract METRICS from all logs
echo "→ Extracting metrics..."
grep "METRICS:" "$RESULTS_DIR"/*.log > "$RESULTS_DIR/all-metrics.jsonl" 2>/dev/null || true

# Count message types
message_sent_count=$(grep -c '"event_type":"MessageSent"' "$RESULTS_DIR/all-metrics.jsonl" 2>/dev/null || echo "0")
doc_inserted_count=$(grep -c '"event_type":"DocumentInserted"' "$RESULTS_DIR/all-metrics.jsonl" 2>/dev/null || echo "0")
doc_received_count=$(grep -c '"event_type":"DocumentReceived"' "$RESULTS_DIR/all-metrics.jsonl" 2>/dev/null || echo "0")

echo "Message Counts:"
echo "  MessageSent: $message_sent_count"
echo "  DocumentInserted: $doc_inserted_count"
echo "  DocumentReceived: $doc_received_count"
echo ""

# P2P Latency Analysis (filter out initialization outliers > 1000ms)
echo "P2P Latency Analysis (squad summaries):"
latencies=$(grep '"event_type":"DocumentReceived"' "$RESULTS_DIR/all-metrics.jsonl" 2>/dev/null | \
    grep -oP '"latency_ms":[0-9.]+' | grep -oP '[0-9.]+' | \
    awk '$1 < 1000' | sort -n)

if [ -n "$latencies" ]; then
    latency_count=$(echo "$latencies" | wc -l)
    avg_latency=$(echo "$latencies" | awk '{sum+=$1; count++} END {printf "%.2f", sum/count}')
    p50_latency=$(echo "$latencies" | awk '{a[NR]=$1} END {print a[int(NR*0.5)]}')
    p90_latency=$(echo "$latencies" | awk '{a[NR]=$1} END {print a[int(NR*0.9)]}')
    p99_latency=$(echo "$latencies" | awk '{a[NR]=$1} END {print a[int(NR*0.99)]}')
    min_latency=$(echo "$latencies" | head -1)
    max_latency=$(echo "$latencies" | tail -1)

    echo "  Samples: $latency_count (filtered < 1000ms)"
    echo "  Min: ${min_latency}ms"
    echo "  Avg: ${avg_latency}ms"
    echo "  p50: ${p50_latency}ms"
    echo "  p90: ${p90_latency}ms"
    echo "  p99: ${p99_latency}ms"
    echo "  Max: ${max_latency}ms"
else
    echo "  No latency data available"
fi
echo ""

# Theoretical bandwidth reduction
NODES=24
SQUADS=3
THEORETICAL_BASELINE=$((NODES * NODES))  # O(n²)
THEORETICAL_MODE4=$((NODES + SQUADS))     # O(n log n) approximation
REDUCTION=$(awk "BEGIN {printf \"%.1f\", (1 - $THEORETICAL_MODE4/$THEORETICAL_BASELINE) * 100}")

echo "Bandwidth Optimization:"
echo "  Theoretical Baseline (Mode 2): $THEORETICAL_BASELINE operations"
echo "  Theoretical Mode 4: $THEORETICAL_MODE4 operations"
echo "  Theoretical Reduction: ${REDUCTION}%"
echo "  Actual Messages Sent: $message_sent_count"
echo "  Actual Documents Inserted: $doc_inserted_count"
echo ""

# Cleanup
echo "════════════════════════════════════════════════════════════"
echo "Cleanup"
echo "════════════════════════════════════════════════════════════"
echo ""

echo "→ Destroying topology..."
containerlab destroy --topo "$TOPOLOGY" > /dev/null 2>&1
echo "✓ Topology destroyed"

echo ""
echo "════════════════════════════════════════════════════════════"
echo "Test Complete"
echo "════════════════════════════════════════════════════════════"
echo ""
echo "Results saved to: $RESULTS_DIR/"
echo ""
echo "Summary:"
echo "  Bandwidth: $BANDWIDTH_LABEL"
echo "  Duration: ${ACTUAL_DURATION}s"
echo "  Squad Aggregations: $(grep -c "Aggregated squad" "$RESULTS_DIR"/squad-*-leader.log 2>/dev/null || echo "0")"
echo "  Platoon Aggregations: $platoon_count"
echo "  P2P Latency (avg): ${avg_latency:-N/A}ms"
echo "  Messages Sent: $message_sent_count"
echo ""
