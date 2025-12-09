#!/bin/bash
# Lab 4-automerge: Hierarchical HIVE CRDT with AutomergeIroh Backend
#
# Tests hierarchical topology with AutomergeIroh CRDT (squad → platoon → company hierarchy)
# Compares to Lab 4 Ditto results to measure backend performance differences

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

source ./test-common.sh

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="/work/hive-sim-results/automerge-hierarchical-${TIMESTAMP}"
RESULTS_CSV="${RESULTS_DIR}/automerge-hierarchical-results.csv"

# Hierarchical scales: same as Lab 4 Ditto for direct comparison
# Skip 1000n as it fails with Ditto too (containerlab/Docker limitations)
NODE_COUNTS=(24 48 96 384)
BANDWIDTHS=("1gbps" "100mbps" "1mbps" "256kbps")
TEST_DURATION_SECS=120  # 2 minutes per test

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Lab 4-automerge: Hierarchical HIVE CRDT with AutomergeIroh ║"
echo "║  Multi-tier topology with aggregation                       ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results: ${RESULTS_DIR}"
echo ""

# AutomergeIroh doesn't need Ditto credentials
echo "Validating environment (AutomergeIroh mode - no Ditto credentials needed)..."
if ! command -v docker &> /dev/null; then
    echo -e "\033[0;31mERROR: docker not found\033[0m"
    exit 1
fi

if ! command -v containerlab &> /dev/null; then
    echo -e "\033[0;31mERROR: containerlab not found\033[0m"
    exit 1
fi

if ! docker images | grep -q "hive-sim-node"; then
    echo -e "\033[0;31mERROR: hive-sim-node:latest image not found\033[0m"
    echo "Run: docker build -t hive-sim-node:latest -f hive-sim/Dockerfile ."
    exit 1
fi

echo -e "\033[0;32mEnvironment validated\033[0m"
echo ""

mkdir -p "${RESULTS_DIR}/logs"

# Pre-test cleanup: remove any stale lab4 Docker networks that could cause subnet conflicts
echo "Cleaning up any stale Docker networks..."
docker network ls --format '{{.Name}}' | grep -E "(lab4-automerge|lab4-hierarchical)" | xargs -r docker network rm 2>/dev/null || true

# CSV header with hierarchical metrics (same as Lab 4 Ditto for comparison)
echo "NodeCount,Bandwidth,Topology,Soldier_to_Squad_P50_ms,Soldier_to_Squad_P95_ms,Squad_to_Platoon_P50_ms,Squad_to_Platoon_P95_ms,Aggregation_Ratio,Total_Ops,Status" > "$RESULTS_CSV"

TOTAL_TESTS=$((${#NODE_COUNTS[@]} * ${#BANDWIDTHS[@]}))
CURRENT_TEST=0

for NODE_COUNT in "${NODE_COUNTS[@]}"; do
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "Node Count: ${NODE_COUNT} (hierarchical topology + AutomergeIroh)"
    echo "═══════════════════════════════════════════════════════════"

    # Determine topology structure based on node count
    case $NODE_COUNT in
        24)
            TOPOLOGY="1_platoon_3_squads"
            ;;
        48)
            TOPOLOGY="2_platoons_6_squads"
            ;;
        96)
            TOPOLOGY="4_platoons_12_squads"
            ;;
        384)
            TOPOLOGY="multi_company_384n"
            ;;
    esac

    for BANDWIDTH in "${BANDWIDTHS[@]}"; do
        CURRENT_TEST=$((CURRENT_TEST + 1))
        TEST_NAME="lab4-automerge-${NODE_COUNT}n-${BANDWIDTH}"

        echo "  [${CURRENT_TEST}/${TOTAL_TESTS}] ${TEST_NAME}"

        # Generate topology with Automerge backend
        TOPO_FILE="${RESULTS_DIR}/${TEST_NAME}.yaml"
        echo "    Generating ${NODE_COUNT}-node hierarchical topology (Automerge)..."

        python3 generate-lab4-hierarchical-topology.py \
            --nodes "${NODE_COUNT}" \
            --bandwidth "${BANDWIDTH}" \
            --backend automerge \
            --output "${TOPO_FILE}" || {
            echo "    ⚠️  Failed to generate topology"
            echo "${NODE_COUNT},${BANDWIDTH},${TOPOLOGY},0,0,0,0,0,0,FAIL" >> "$RESULTS_CSV"
            continue
        }

        # Get lab name and count expected nodes from topology file
        LAB_NAME=$(grep '^name:' "$TOPO_FILE" | awk '{print $2}')
        EXPECTED_NODES=$(grep -c "kind: linux" "$TOPO_FILE")

        # Deploy topology
        echo "    Deploying topology (${EXPECTED_NODES} nodes)..."
        if ! containerlab deploy -t "$TOPO_FILE" --reconfigure > /dev/null 2>&1; then
            echo "    ❌ Deployment failed"
            echo "${NODE_COUNT},${BANDWIDTH},${TOPOLOGY},0,0,0,0,0,0,FAIL" >> "$RESULTS_CSV"
            continue
        fi

        # Wait for all containers to be running
        # Scale wait time based on node count: 2 min base + 1 min per 100 nodes
        MAX_WAIT=$(( 60 + (EXPECTED_NODES / 100) * 30 ))
        echo "    Waiting for all ${EXPECTED_NODES} containers to be running (max ${MAX_WAIT} iterations)..."
        WAIT_COUNT=0
        while true; do
            RUNNING_COUNT=$(docker ps --filter "name=clab-${LAB_NAME}" --format "{{.Names}}" 2>/dev/null | wc -l)
            if [ "$RUNNING_COUNT" -ge "$EXPECTED_NODES" ]; then
                echo "    All ${RUNNING_COUNT} containers running"
                break
            fi
            WAIT_COUNT=$((WAIT_COUNT + 1))
            if [ "$WAIT_COUNT" -ge "$MAX_WAIT" ]; then
                echo "    ❌ FAIL: Timeout waiting for containers (${RUNNING_COUNT}/${EXPECTED_NODES})"
                echo "${NODE_COUNT},${BANDWIDTH},${TOPOLOGY},0,0,0,0,0,0,FAIL" >> "$RESULTS_CSV"
                containerlab destroy -t "$TOPO_FILE" --cleanup > /dev/null 2>&1 || true
                continue 2
            fi
            sleep 2
        done

        # Wait for nodes to initialize and generate metrics
        echo "    Running test for ${TEST_DURATION_SECS}s..."
        sleep "$TEST_DURATION_SECS"

        # Collect logs from different tiers
        LOG_DIR="${RESULTS_DIR}/logs/${TEST_NAME}"
        mkdir -p "$LOG_DIR"

        # Collect soldier logs (sample)
        SOLDIER_CONTAINER=$(docker ps --format '{{.Names}}' | grep "${LAB_NAME}" | grep -E "soldier" | head -1)
        if [ -n "$SOLDIER_CONTAINER" ]; then
            docker logs "$SOLDIER_CONTAINER" 2>&1 > "${LOG_DIR}/soldier-sample.log"
        fi

        # Collect squad leader logs (all squad leaders)
        docker ps --format '{{.Names}}' | grep "${LAB_NAME}" | grep -E "squad.*leader" | while read CONTAINER; do
            LEADER_NAME=$(echo "$CONTAINER" | sed 's/clab-//' | sed "s/${LAB_NAME}-//")
            docker logs "$CONTAINER" 2>&1 > "${LOG_DIR}/${LEADER_NAME}.log"
        done

        # Collect platoon leader logs
        docker ps --format '{{.Names}}' | grep "${LAB_NAME}" | grep -E "platoon.*leader" | while read CONTAINER; do
            LEADER_NAME=$(echo "$CONTAINER" | sed 's/clab-//' | sed "s/${LAB_NAME}-//")
            docker logs "$CONTAINER" 2>&1 > "${LOG_DIR}/${LEADER_NAME}.log"
        done

        # Collect company commander logs
        docker ps --format '{{.Names}}' | grep "${LAB_NAME}" | grep -E "commander" | while read CONTAINER; do
            LEADER_NAME=$(echo "$CONTAINER" | sed 's/clab-//' | sed "s/${LAB_NAME}-//")
            docker logs "$CONTAINER" 2>&1 > "${LOG_DIR}/${LEADER_NAME}.log"
        done

        # Extract CROSS-TIER PROPAGATION latencies
        SOLDIER_TO_SQUAD_P50=0
        SOLDIER_TO_SQUAD_P95=0
        SQUAD_TO_PLATOON_P50=0
        SQUAD_TO_PLATOON_P95=0
        AGGREGATION_RATIO=0
        TOTAL_OPS=0
        STATUS="PASS"

        # Parse Soldier → Squad aggregation latency from CRDTUpsert events
        cat "${LOG_DIR}"/*squad*leader*.log 2>/dev/null | \
            grep 'METRICS:' | \
            grep '"event_type":"CRDTUpsert"' | \
            grep '"tier":"squad_leader"' | \
            sed 's/.*"latency_ms":\([0-9.]*\).*/\1/' | \
            sort -n > /tmp/soldier_to_squad_lat_$$.txt || true

        # Alternative: DocumentReceived events
        if [ ! -s /tmp/soldier_to_squad_lat_$$.txt ]; then
            cat "${LOG_DIR}"/*squad*leader*.log 2>/dev/null | \
                grep 'METRICS:' | \
                grep '"event_type":"DocumentReceived"' | \
                grep -v '"latency_type":"creation"' | \
                sed 's/.*"latency_ms":\([0-9.]*\).*/\1/' | \
                sort -n > /tmp/soldier_to_squad_lat_$$.txt || true
        fi

        if [ -s /tmp/soldier_to_squad_lat_$$.txt ]; then
            SOLDIER_TO_SQUAD_P50=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) print a[int(c*0.5)]; else print 0}' /tmp/soldier_to_squad_lat_$$.txt)
            SOLDIER_TO_SQUAD_P95=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) print a[int(c*0.95)]; else print 0}' /tmp/soldier_to_squad_lat_$$.txt)
        fi
        rm -f /tmp/soldier_to_squad_lat_$$.txt

        # Parse Squad → Platoon propagation latency
        cat "${LOG_DIR}"/*platoon*leader*.log 2>/dev/null | \
            grep 'METRICS:' | \
            grep '"event_type":"CRDTUpsert"' | \
            grep '"tier":"platoon_leader"' | \
            sed 's/.*"latency_ms":\([0-9.]*\).*/\1/' | \
            sort -n > /tmp/squad_to_platoon_lat_$$.txt || true

        # Alternative: DocumentReceived events
        if [ ! -s /tmp/squad_to_platoon_lat_$$.txt ]; then
            cat "${LOG_DIR}"/*platoon*leader*.log 2>/dev/null | \
                grep 'METRICS:' | \
                grep '"event_type":"DocumentReceived"' | \
                grep -v '"latency_type":"creation"' | \
                sed 's/.*"latency_ms":\([0-9.]*\).*/\1/' | \
                sort -n > /tmp/squad_to_platoon_lat_$$.txt || true
        fi

        if [ -s /tmp/squad_to_platoon_lat_$$.txt ]; then
            SQUAD_TO_PLATOON_P50=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) print a[int(c*0.5)]; else print 0}' /tmp/squad_to_platoon_lat_$$.txt)
            SQUAD_TO_PLATOON_P95=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) print a[int(c*0.95)]; else print 0}' /tmp/squad_to_platoon_lat_$$.txt)
        fi
        rm -f /tmp/squad_to_platoon_lat_$$.txt

        # Calculate aggregation efficiency from input_count in AggregationCompleted events
        cat "${LOG_DIR}"/*squad*leader*.log 2>/dev/null | \
            grep 'METRICS:' | \
            grep '"event_type":"AggregationCompleted"' | \
            grep '"tier":"squad"' | \
            sed 's/.*"input_count":\([0-9]*\).*/\1/' | \
            head -1 > /tmp/agg_ratio_$$.txt || true

        if [ -s /tmp/agg_ratio_$$.txt ]; then
            AGGREGATION_RATIO=$(cat /tmp/agg_ratio_$$.txt)
        fi
        rm -f /tmp/agg_ratio_$$.txt

        # Count total operations (soldiers + squad leaders + platoon leaders)
        TOTAL_OPS=$(cat "${LOG_DIR}"/*.log 2>/dev/null | grep -c 'METRICS:' || echo "0")

        # Determine status based on whether we got metrics
        if [ "$SQUAD_TO_PLATOON_P50" = "0" ] && [ "$SOLDIER_TO_SQUAD_P50" = "0" ]; then
            STATUS="WARN"
            echo "    ⚠️  No propagation metrics collected"
        else
            echo "    ✅ PASS (Soldier→Squad P95: ${SOLDIER_TO_SQUAD_P95}ms, Squad→Platoon P95: ${SQUAD_TO_PLATOON_P95}ms)"
        fi

        # Write results
        echo "${NODE_COUNT},${BANDWIDTH},${TOPOLOGY},${SOLDIER_TO_SQUAD_P50},${SOLDIER_TO_SQUAD_P95},${SQUAD_TO_PLATOON_P50},${SQUAD_TO_PLATOON_P95},${AGGREGATION_RATIO},${TOTAL_OPS},${STATUS}" >> "$RESULTS_CSV"

        # Cleanup - destroy topology and remove any orphaned Docker networks
        containerlab destroy -t "$TOPO_FILE" --cleanup > /dev/null 2>&1 || true
        # Clean up any stale Docker networks from this test
        docker network ls --format '{{.Name}}' | grep -E "(lab4-automerge|lab4-hierarchical)" | xargs -r docker network rm 2>/dev/null || true
        sleep 2
    done
done

PASSED=$(grep -c ",PASS$" "$RESULTS_CSV" || echo "0")
FAILED=$(grep -c ",FAIL$" "$RESULTS_CSV" || echo "0")
WARNED=$(grep -c ",WARN$" "$RESULTS_CSV" || echo "0")

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  Lab 4-automerge: Hierarchical AutomergeIroh Tests Complete   ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Results: ${RESULTS_DIR}"
echo "Passed: ${PASSED} / Warnings: ${WARNED} / Failed: ${FAILED}"
echo ""
echo "Lab 4-automerge tests hierarchical topology WITH AutomergeIroh CRDT"
echo "Compare to Lab 4 Ditto to measure backend performance differences"
echo ""
echo "Analysis:"
echo "  - Soldier→Squad latency: Cross-tier CRDT propagation from soldier to squad leader"
echo "  - Squad→Platoon latency: Cross-tier CRDT propagation from squad to platoon leader"
echo "  - Aggregation ratio: Documents reduced at each tier"
echo ""

# Generate comparison if Lab 4 Ditto results exist
if ls hive-hierarchical-*/hive-hierarchical-results.csv 2>/dev/null | head -1 > /dev/null; then
    DITTO_RESULTS=$(ls -t hive-hierarchical-*/hive-hierarchical-results.csv | head -1)
    echo "Comparison to Lab 4 Ditto:"
    echo "  Ditto results: ${DITTO_RESULTS}"
    echo "  Automerge results: ${RESULTS_CSV}"
    echo ""
    echo "Run: python3 compare-lab4-ditto-vs-automerge.py ${DITTO_RESULTS} ${RESULTS_CSV}"
fi
