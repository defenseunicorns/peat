#!/bin/bash
# Lab 1: Producer-Only Baseline Tests
#
# Tests pure server ingress capacity (clients upload to server, NO broadcast)
#
# Expected: Better scaling than Lab 2 (full replication) since there's no O(n²) broadcast overhead
# - Lab 2 breaks at 384-500 nodes
# - Lab 1 should handle 500-1000 nodes (O(n) ingress only)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Source common test functions
source ./test-common.sh

# Test configuration
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="producer-only-${TIMESTAMP}"
RESULTS_CSV="${RESULTS_DIR}/producer-only-results.csv"

# Test matrix: 8 node counts × 4 bandwidths = 32 tests
NODE_COUNTS=(24 48 96 192 384 500 750 1000)
BANDWIDTHS=("1gbps" "100mbps" "1mbps" "256kbps")

TEST_DURATION_SECS=120  # 2 minutes per test
UPDATE_FREQUENCY=5      # Client update every 5 seconds

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Producer-Only Baseline Tests (Lab 1)                    ║"
echo "║  Upload-only: Clients → Server (NO broadcast)            ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results directory: ${RESULTS_DIR}"
echo ""

# Validate environment
validate_environment

# Create results directory
mkdir -p "${RESULTS_DIR}/logs"

# CSV header
echo "NodeCount,Bandwidth,Ingress_P50_ms,Ingress_P95_ms,Ingress_P99_ms,Ingress_Max_ms,Server_TotalMessages,Status" > "$RESULTS_CSV"

# Count total tests
TOTAL_TESTS=$((${#NODE_COUNTS[@]} * ${#BANDWIDTHS[@]}))
CURRENT_TEST=0

echo "Running ${TOTAL_TESTS} producer-only baseline tests (NO CRDT, upload-only)..."
echo ""

for NODE_COUNT in "${NODE_COUNTS[@]}"; do
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "Node Count: ${NODE_COUNT}"
    echo "═══════════════════════════════════════════════════════════"

    for BANDWIDTH in "${BANDWIDTHS[@]}"; do
        CURRENT_TEST=$((CURRENT_TEST + 1))
        TEST_NAME="producer-only-${NODE_COUNT}n-${BANDWIDTH}"

        echo "  [${CURRENT_TEST}/${TOTAL_TESTS}] ${TEST_NAME}"

        # Generate topology
        TOPO_FILE="${RESULTS_DIR}/${TEST_NAME}.yaml"
        python3 - <<EOF
#!/usr/bin/env python3
import yaml

# Producer-only hub-spoke topology
topology = {
    'name': '${TEST_NAME}',
    'topology': {
        'nodes': {},
        'links': []
    }
}

# Server node (aggregator, no broadcast)
topology['topology']['nodes']['server'] = {
    'kind': 'linux',
    'image': 'hive-sim-node:latest',
    'env': {
        'NODE_ID': 'server',
        'MODE': 'writer',  # Server mode
        'TCP_LISTEN': '12345',
        'USE_PRODUCER_ONLY': 'true',
        'UPDATE_FREQUENCY_SECS': '${UPDATE_FREQUENCY}',
        'BANDWIDTH': '${BANDWIDTH}'
    }
}

# Client nodes (producers)
for i in range(1, ${NODE_COUNT} + 1):
    node_id = f'client-{i}'
    topology['topology']['nodes'][node_id] = {
        'kind': 'linux',
        'image': 'hive-sim-node:latest',
        'env': {
            'NODE_ID': node_id,
            'MODE': 'reader',  # Client mode
            'TCP_CONNECT': 'server:12345',
            'USE_PRODUCER_ONLY': 'true',
            'UPDATE_FREQUENCY_SECS': '${UPDATE_FREQUENCY}',
            'BANDWIDTH': '${BANDWIDTH}'
        }
    }

    # Link client to server
    topology['topology']['links'].append({
        'endpoints': [node_id + ':eth1', 'server:eth' + str(i)]
    })

with open('${TOPO_FILE}', 'w') as f:
    yaml.dump(topology, f, default_flow_style=False, sort_keys=False)
EOF

        # Deploy topology
        containerlab deploy -t "$TOPO_FILE" --reconfigure > /dev/null 2>&1

        # Wait for test to run
        echo "    Running for ${TEST_DURATION_SECS}s..."
        sleep "$TEST_DURATION_SECS"

        # Collect logs
        LOG_DIR="${RESULTS_DIR}/logs"
        mkdir -p "$LOG_DIR"

        # Server log
        docker logs clab-${TEST_NAME}-server 2>&1 > "${LOG_DIR}/${TEST_NAME}-server.log"

        # Sample client logs (collect first 5 clients for debugging)
        for i in $(seq 1 5); do
            if [ $i -le $NODE_COUNT ]; then
                docker logs clab-${TEST_NAME}-client-${i} 2>&1 > "${LOG_DIR}/${TEST_NAME}-client-${i}.log" || true
            fi
        done

        # Extract metrics
        INGRESS_P50=0
        INGRESS_P95=0
        INGRESS_P99=0
        INGRESS_MAX=0
        SERVER_TOTAL_MESSAGES=0
        STATUS="PASS"

        # Extract ingress latencies from server log
        if grep -q "METRICS.*IngressReceived" "${LOG_DIR}/${TEST_NAME}-server.log"; then
            grep "METRICS.*IngressReceived" "${LOG_DIR}/${TEST_NAME}-server.log" | \
                grep -o '"latency_ms":[0-9.]*' | \
                cut -d: -f2 | sort -n > /tmp/ingress_lat_$$.txt

            if [ -s /tmp/ingress_lat_$$.txt ]; then
                INGRESS_P50=$(awk 'BEGIN{c=0} {sum+=$1; a[c++]=$1} END{print a[int(c*0.5)]}' /tmp/ingress_lat_$$.txt)
                INGRESS_P95=$(awk 'BEGIN{c=0} {sum+=$1; a[c++]=$1} END{print a[int(c*0.95)]}' /tmp/ingress_lat_$$.txt)
                INGRESS_P99=$(awk 'BEGIN{c=0} {sum+=$1; a[c++]=$1} END{print a[int(c*0.99)]}' /tmp/ingress_lat_$$.txt)
                INGRESS_MAX=$(tail -1 /tmp/ingress_lat_$$.txt)
            fi
            rm -f /tmp/ingress_lat_$$.txt
        else
            STATUS="FAIL"
        fi

        # Extract server total messages from last ServerStats event
        if grep -q "METRICS.*ServerStats" "${LOG_DIR}/${TEST_NAME}-server.log"; then
            SERVER_TOTAL_MESSAGES=$(grep "METRICS.*ServerStats" "${LOG_DIR}/${TEST_NAME}-server.log" | \
                tail -1 | grep -o '"total_messages_received":[0-9]*' | cut -d: -f2)
        fi

        # Write results to CSV
        echo "${NODE_COUNT},${BANDWIDTH},${INGRESS_P50},${INGRESS_P95},${INGRESS_P99},${INGRESS_MAX},${SERVER_TOTAL_MESSAGES},${STATUS}" >> "$RESULTS_CSV"

        # Cleanup
        containerlab destroy -t "$TOPO_FILE" --cleanup > /dev/null 2>&1

        if [ "$STATUS" = "PASS" ]; then
            echo "    ✅ PASS"
        else
            echo "    ❌ FAIL"
        fi
    done
done

# Count results
PASSED=$(grep -c ",PASS$" "$RESULTS_CSV" || echo "0")
FAILED=$(grep -c ",FAIL$" "$RESULTS_CSV" || echo "0")

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Producer-Only Baseline Tests Complete                   ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results: ${RESULTS_DIR}"
echo "Passed: ${PASSED} / Failed: ${FAILED}"
echo ""
echo "Note: Producer-only tests upload to server with NO broadcast back"
echo ""

# Run analysis
echo "Running scaling analysis..."
python3 analyze-producer-only-scaling.py "${RESULTS_DIR}" > "${RESULTS_DIR}/analysis.txt"

echo ""
echo "✅ Analysis complete: ${RESULTS_DIR}/analysis.txt"
