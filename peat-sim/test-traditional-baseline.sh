#!/usr/bin/env bash
set -euo pipefail

#####################################################################
# Traditional Baseline Tests
#
# Pure client-server architecture with NO CRDT
# Uses traditional_baseline binary (periodic full state messages)
#
# Results: traditional-baseline-TIMESTAMP/
# Tests: 8 scales × 4 bandwidths = 32 tests (~1-2 hrs)
# Scales: 24, 48, 96, 192, 384, 500, 750, 1000 nodes
#####################################################################

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Traditional Baseline Tests (NO CRDT)                     ║"
echo "║  Hub-Spoke Client-Server (O(n²) scaling)                  ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Create timestamped results directory
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="traditional-baseline-${TIMESTAMP}"
mkdir -p "$RESULTS_DIR"

echo "Results directory: $RESULTS_DIR"
echo ""

# Source common test functions
source ./test-common.sh

# Load Ditto credentials (needed for other tests, but not used here)
validate_environment || exit 1

# Initialize CSV - now tracking both broadcast and E2E propagation latencies
RESULTS_CSV="$RESULTS_DIR/traditional-results.csv"
echo "NodeCount,Topology,Bandwidth,Broadcast_P50_ms,Broadcast_P95_ms,Broadcast_P99_ms,Broadcast_Max_ms,E2E_P50_ms,E2E_P95_ms,E2E_P99_ms,E2E_Max_ms,Status" > "$RESULTS_CSV"

# Test parameters - NO BACKEND (traditional uses raw TCP)
NODE_COUNTS=(24 48 96 192 384 500 750 1000)
BANDWIDTHS=("1gbps" "100mbps" "1mbps" "256kbps")

TOTAL_TESTS=$((${#NODE_COUNTS[@]} * ${#BANDWIDTHS[@]}))
CURRENT_TEST=0
PASS_COUNT=0
FAIL_COUNT=0

echo "Running ${TOTAL_TESTS} traditional baseline tests (NO CRDT)..."
echo ""

for NODE_COUNT in "${NODE_COUNTS[@]}"; do
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "Node Count: $NODE_COUNT"
    echo "═══════════════════════════════════════════════════════════"

    TOPOLOGY_FILE="topologies/traditional-battalion-${NODE_COUNT}node.yaml"

    if [ ! -f "$TOPOLOGY_FILE" ]; then
        echo "  ⚠️  Topology not found: $TOPOLOGY_FILE"
        continue
    fi

    for BW in "${BANDWIDTHS[@]}"; do
        CURRENT_TEST=$((CURRENT_TEST + 1))
        TEST_NAME="traditional-${NODE_COUNT}n-${BW}"

        echo "  [$CURRENT_TEST/$TOTAL_TESTS] $TEST_NAME"

        TEST_DIR="$RESULTS_DIR/$TEST_NAME"
        LOG_FILE="$RESULTS_DIR/${TEST_NAME}.log"

        # Export bandwidth for containerlab to apply via tc
        export BANDWIDTH="$BW"

        if ./test-scaling-validation.sh "$NODE_COUNT" "$TOPOLOGY_FILE" "$TEST_DIR" > "$LOG_FILE" 2>&1; then
            # Extract metrics from container logs
            SCALING_DIR=$(find . -name "scaling-results-${NODE_COUNT}node-*" -type d -mmin -5 | sort -r | head -1)

            if [ -d "$SCALING_DIR/logs" ]; then
                # Extract MessageReceived latencies (server broadcast efficiency)
                grep -h 'METRICS.*MessageReceived' "$SCALING_DIR/logs"/*.log 2>/dev/null | \
                    grep -o '"latency_us":[0-9.]*' | \
                    cut -d: -f2 | \
                    awk '{print $1/1000}' | \
                    sort -n > /tmp/broadcast_lat_$$.txt

                # Extract PropagationReceived latencies (end-to-end client-to-client)
                grep -h 'METRICS.*PropagationReceived' "$SCALING_DIR/logs"/*.log 2>/dev/null | \
                    grep -o '"propagation_latency_ms":[0-9.]*' | \
                    cut -d: -f2 | \
                    sort -n > /tmp/e2e_lat_$$.txt

                # Calculate broadcast latency percentiles
                if [ -s /tmp/broadcast_lat_$$.txt ]; then
                    TOTAL=$(wc -l < /tmp/broadcast_lat_$$.txt)
                    P50_LINE=$((TOTAL * 50 / 100))
                    P95_LINE=$((TOTAL * 95 / 100))
                    P99_LINE=$((TOTAL * 99 / 100))

                    B_P50=$(sed -n "${P50_LINE}p" /tmp/broadcast_lat_$$.txt)
                    B_P95=$(sed -n "${P95_LINE}p" /tmp/broadcast_lat_$$.txt)
                    B_P99=$(sed -n "${P99_LINE}p" /tmp/broadcast_lat_$$.txt)
                    B_MAX=$(tail -1 /tmp/broadcast_lat_$$.txt)
                else
                    B_P50=0; B_P95=0; B_P99=0; B_MAX=0
                fi

                # Calculate E2E propagation latency percentiles
                if [ -s /tmp/e2e_lat_$$.txt ]; then
                    TOTAL=$(wc -l < /tmp/e2e_lat_$$.txt)
                    P50_LINE=$((TOTAL * 50 / 100))
                    P95_LINE=$((TOTAL * 95 / 100))
                    P99_LINE=$((TOTAL * 99 / 100))

                    E_P50=$(sed -n "${P50_LINE}p" /tmp/e2e_lat_$$.txt)
                    E_P95=$(sed -n "${P95_LINE}p" /tmp/e2e_lat_$$.txt)
                    E_P99=$(sed -n "${P99_LINE}p" /tmp/e2e_lat_$$.txt)
                    E_MAX=$(tail -1 /tmp/e2e_lat_$$.txt)
                else
                    E_P50=0; E_P95=0; E_P99=0; E_MAX=0
                fi

                echo "$NODE_COUNT,traditional,$BW,$B_P50,$B_P95,$B_P99,$B_MAX,$E_P50,$E_P95,$E_P99,$E_MAX,PASS" >> "$RESULTS_CSV"
                rm -f /tmp/broadcast_lat_$$.txt /tmp/e2e_lat_$$.txt
            else
                echo "$NODE_COUNT,traditional,$BW,0,0,0,0,0,0,0,0,PASS" >> "$RESULTS_CSV"
            fi
            
            PASS_COUNT=$((PASS_COUNT + 1))
            echo "    ✅ PASS"
        else
            echo "$NODE_COUNT,traditional,$BW,0,0,0,0,0,0,0,FAIL" >> "$RESULTS_CSV"
            FAIL_COUNT=$((FAIL_COUNT + 1))
            echo "    ❌ FAIL"
        fi

        unset BANDWIDTH
        sleep 2
    done
done

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Traditional Baseline Tests Complete                      ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results: $RESULTS_DIR"
echo "Passed: $PASS_COUNT / Failed: $FAIL_COUNT"
echo ""
echo "Note: Traditional baseline uses NO CRDT (pure TCP client-server)"
