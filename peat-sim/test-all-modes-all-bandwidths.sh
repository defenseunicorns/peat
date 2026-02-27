#!/bin/bash
# Comprehensive CAP Protocol Test Suite
# Tests all modes (1-4) across all bandwidth constraints (1Gbps, 100Mbps, 1Mbps, 256Kbps)
#
# Total tests: 4 modes × 4 bandwidths = 16 test runs
# Estimated time: ~40-60 minutes (depends on convergence times)

set -e

cd "$(dirname "$0")"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  CAP Protocol - Comprehensive Bandwidth Test Suite        ║"
echo "║  All Modes × All Bandwidth Constraints                    ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Load environment variables
set -a
source ../.env 2>/dev/null || true
set +a

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
SUITE_DIR="test-suite-all-modes-bandwidths-${TIMESTAMP}"
mkdir -p "$SUITE_DIR"

REPORT_FILE="${SUITE_DIR}/COMPREHENSIVE_REPORT.md"

echo "Suite Directory: $SUITE_DIR"
echo "Report File: $REPORT_FILE"
echo ""

# Bandwidth configurations to test
BANDWIDTHS=("1gbps" "100mbps" "1mbps" "256kbps")

# Mode configurations
declare -A MODES=(
    ["mode1"]="topologies/squad-12node-client-server.yaml:cap-squad-client-server:12"
    ["mode2"]="topologies/squad-12node-hub-spoke.yaml:cap-squad-hub-spoke:12"
    ["mode3"]="topologies/squad-12node-dynamic-mesh.yaml:cap-squad-dynamic-mesh:12"
    ["mode4"]="topologies/platoon-24node-mesh-mode4.yaml:cap-platoon-mode4-mesh:24"
)

# Bandwidth values in Kbps
declare -A BW_KBPS=(
    ["1gbps"]=1048576
    ["100mbps"]=102400
    ["1mbps"]=1024
    ["256kbps"]=256
)

# Test duration per mode (seconds)
declare -A TEST_DURATIONS=(
    ["1gbps"]=60
    ["100mbps"]=60
    ["1mbps"]=90
    ["256kbps"]=90
)

# Initialize report
cat > "$REPORT_FILE" << 'EOF'
# CAP Protocol - Comprehensive Bandwidth Test Suite

**Test Date:** $(date)
**Test Suite:** All Modes × All Bandwidth Constraints
**Total Tests:** 16 (4 modes × 4 bandwidths)

---

## Executive Summary

This report validates CAP protocol performance across all operating modes under various bandwidth constraints, from gigabit ethernet to tactical radio bandwidths.

### Test Matrix

| Mode | 1Gbps | 100Mbps | 1Mbps | 256Kbps |
|------|-------|---------|-------|---------|
| Mode 1 (Client-Server) | ⏳ | ⏳ | ⏳ | ⏳ |
| Mode 2 (Hub-Spoke) | ⏳ | ⏳ | ⏳ | ⏳ |
| Mode 3 (Dynamic Mesh) | ⏳ | ⏳ | ⏳ | ⏳ |
| Mode 4 (Hierarchical) | ⏳ | ⏳ | ⏳ | ⏳ |

---

EOF

sed -i "s/\$(date)/$(date)/" "$REPORT_FILE"

# Function to apply bandwidth constraints
apply_bandwidth() {
    local rate_kbps=$1
    local lab_name=$2

    echo "  → Applying ${rate_kbps} Kbps constraint..."

    local containers=$(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}")
    for container in $containers; do
        containerlab tools netem set -n "$container" -i eth0 --rate "$rate_kbps" 2>/dev/null || true
    done
}

# Function to test one mode at one bandwidth
test_mode_bandwidth() {
    local mode_name=$1
    local bandwidth=$2
    local topology=$3
    local lab_name=$4
    local node_count=$5

    local rate_kbps=${BW_KBPS[$bandwidth]}
    local duration=${TEST_DURATIONS[$bandwidth]}

    local test_dir="${SUITE_DIR}/${mode_name}-${bandwidth}"
    mkdir -p "$test_dir"

    echo ""
    echo "════════════════════════════════════════════════════════════"
    echo "Testing: ${mode_name} @ ${bandwidth}"
    echo "════════════════════════════════════════════════════════════"
    echo "Topology: $topology"
    echo "Bandwidth: $rate_kbps Kbps"
    echo "Duration: ${duration}s"
    echo "Nodes: $node_count"
    echo ""

    # Deploy topology
    echo "→ Deploying topology..."
    containerlab deploy --topo "$topology" > /dev/null 2>&1

    echo "→ Waiting 30s for network formation..."
    sleep 30

    # Apply bandwidth constraint
    apply_bandwidth "$rate_kbps" "$lab_name"

    echo "→ Waiting 10s for constraint stabilization..."
    sleep 10

    # Run test
    echo "→ Running ${duration}s test..."
    local start_time=$(date +%s)

    # Progress indicator
    for i in $(seq 1 $duration); do
        if [ $((i % 15)) -eq 0 ]; then
            echo "  [T=${i}s/${duration}s]"
        fi
        sleep 1
    done

    local end_time=$(date +%s)
    local actual_duration=$((end_time - start_time))

    # Collect logs
    echo "→ Collecting logs..."
    for container in $(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}"); do
        node_name=$(echo "$container" | sed "s/clab-${lab_name}-//")
        docker logs "$container" 2>&1 > "${test_dir}/${node_name}.log"
    done

    # Extract metrics
    grep "METRICS:" "${test_dir}"/*.log > "${test_dir}/all-metrics.jsonl" 2>/dev/null || true

    # Analyze results
    local msg_sent=$(grep -c '"event_type":"MessageSent"' "${test_dir}/all-metrics.jsonl" 2>/dev/null || echo "0")
    local doc_inserted=$(grep -c '"event_type":"DocumentInserted"' "${test_dir}/all-metrics.jsonl" 2>/dev/null || echo "0")
    local doc_received=$(grep -c '"event_type":"DocumentReceived"' "${test_dir}/all-metrics.jsonl" 2>/dev/null || echo "0")

    # Calculate latency stats (filter < 1000ms to remove initialization)
    local latencies=$(grep '"event_type":"DocumentReceived"' "${test_dir}/all-metrics.jsonl" 2>/dev/null | \
        grep -oP '"latency_ms":[0-9.]+' | grep -oP '[0-9.]+' | \
        awk '$1 < 1000' | sort -n)

    local avg_latency="N/A"
    local p50_latency="N/A"
    local p90_latency="N/A"

    if [ -n "$latencies" ]; then
        avg_latency=$(echo "$latencies" | awk '{sum+=$1; count++} END {printf "%.2f", sum/count}')
        p50_latency=$(echo "$latencies" | awk '{a[NR]=$1} END {print a[int(NR*0.5)]}')
        p90_latency=$(echo "$latencies" | awk '{a[NR]=$1} END {print a[int(NR*0.9)]}')
    fi

    # Mode 4 specific: count aggregations
    local squad_agg="N/A"
    local platoon_agg="N/A"
    if [[ "$mode_name" == "mode4" ]]; then
        squad_agg=$(grep -c "Aggregated squad" "${test_dir}"/squad-*-leader.log 2>/dev/null || echo "0")
        platoon_agg=$(grep -c "Aggregated platoon" "${test_dir}"/platoon-leader.log 2>/dev/null || echo "0")
    fi

    # Destroy topology
    echo "→ Destroying topology..."
    containerlab destroy --topo "$topology" > /dev/null 2>&1

    echo "✓ ${mode_name} @ ${bandwidth} complete"
    echo "  Messages: $msg_sent sent, $doc_inserted inserted, $doc_received received"
    echo "  Latency: avg=${avg_latency}ms, p50=${p50_latency}ms, p90=${p90_latency}ms"

    if [[ "$mode_name" == "mode4" ]]; then
        echo "  Aggregations: $squad_agg squad, $platoon_agg platoon"
    fi

    # Append to report
    cat >> "$REPORT_FILE" << EOF

### ${mode_name^^} @ ${bandwidth}

**Test Parameters:**
- Bandwidth: ${rate_kbps} Kbps
- Duration: ${actual_duration}s
- Nodes: ${node_count}

**Results:**
- Messages Sent: ${msg_sent}
- Documents Inserted: ${doc_inserted}
- Documents Received: ${doc_received}
- Avg Latency: ${avg_latency}ms
- p50 Latency: ${p50_latency}ms
- p90 Latency: ${p90_latency}ms

EOF

    if [[ "$mode_name" == "mode4" ]]; then
        cat >> "$REPORT_FILE" << EOF
**Hierarchical Aggregation:**
- Squad Aggregations: ${squad_agg}
- Platoon Aggregations: ${platoon_agg}

EOF
    fi

    # Small delay between tests
    sleep 5
}

# Run all test combinations
total_tests=$((${#MODES[@]} * ${#BANDWIDTHS[@]}))
current_test=0

echo "════════════════════════════════════════════════════════════"
echo "Starting Test Suite"
echo "════════════════════════════════════════════════════════════"
echo "Total Tests: $total_tests"
echo "Estimated Time: 40-60 minutes"
echo ""

START_TIME=$(date +%s)

for mode_name in mode1 mode2 mode3 mode4; do
    IFS=':' read -r topology lab_name node_count <<< "${MODES[$mode_name]}"

    for bandwidth in "${BANDWIDTHS[@]}"; do
        ((current_test++))
        echo ""
        echo "════════════════════════════════════════════════════════════"
        echo "Test $current_test/$total_tests: ${mode_name} @ ${bandwidth}"
        echo "════════════════════════════════════════════════════════════"

        test_mode_bandwidth "$mode_name" "$bandwidth" "$topology" "$lab_name" "$node_count"
    done
done

END_TIME=$(date +%s)
TOTAL_DURATION=$((END_TIME - START_TIME))
TOTAL_MINUTES=$((TOTAL_DURATION / 60))

echo ""
echo "════════════════════════════════════════════════════════════"
echo "Test Suite Complete"
echo "════════════════════════════════════════════════════════════"
echo ""
echo "Total Duration: ${TOTAL_MINUTES} minutes"
echo "Results Directory: $SUITE_DIR"
echo "Report: $REPORT_FILE"
echo ""

# Finalize report
cat >> "$REPORT_FILE" << EOF

---

## Summary

**Total Test Duration:** ${TOTAL_MINUTES} minutes
**Tests Completed:** ${total_tests}
**Results Directory:** \`${SUITE_DIR}\`

**Test Date:** $(date)

EOF

echo "To view the report:"
echo "  cat $REPORT_FILE"
echo ""
