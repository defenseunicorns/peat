#!/bin/bash
# Lab 4: Hierarchical PEAT CRDT
#
# Tests hierarchical topology with PEAT CRDT (squad → platoon → company hierarchy)
# Compares to Lab 3b (flat mesh CRDT) to measure hierarchy benefit

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

source ./test-common.sh

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="peat-hierarchical-${TIMESTAMP}"
RESULTS_CSV="${RESULTS_DIR}/peat-hierarchical-results.csv"

# Hierarchical scales: prove scaling beyond Lab 3b's limits
NODE_COUNTS=(24 48 96 384 1000)
BANDWIDTHS=("1gbps" "100mbps" "1mbps" "256kbps")
TEST_DURATION_SECS=120  # 2 minutes per test

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Lab 4: Hierarchical PEAT CRDT                            ║"
echo "║  Multi-tier topology with aggregation                     ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results: ${RESULTS_DIR}"
echo ""

validate_environment
mkdir -p "${RESULTS_DIR}/logs"

# Pre-test cleanup: remove any stale lab4 Docker networks that could cause subnet conflicts
echo "Cleaning up any stale Docker networks..."
docker network ls --format '{{.Name}}' | grep "lab4-hierarchical" | xargs -r docker network rm 2>/dev/null || true

# CSV header with hierarchical metrics
# Note: Latencies are CROSS-TIER PROPAGATION times (network sync), not local processing
echo "NodeCount,Bandwidth,Topology,Soldier_to_Squad_P50_ms,Soldier_to_Squad_P95_ms,Squad_to_Platoon_P50_ms,Squad_to_Platoon_P95_ms,Aggregation_Ratio,Total_Ops,Status" > "$RESULTS_CSV"

TOTAL_TESTS=$((${#NODE_COUNTS[@]} * ${#BANDWIDTHS[@]}))
CURRENT_TEST=0

for NODE_COUNT in "${NODE_COUNTS[@]}"; do
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "Node Count: ${NODE_COUNT} (hierarchical topology)"
    echo "═══════════════════════════════════════════════════════════"

    # Determine topology structure based on node count
    # Use existing mode4 mesh topologies which have hierarchical aggregation
    case $NODE_COUNT in
        24)
            TOPOLOGY="1_platoon_3_squads"
            TOPO_FILE="topologies/platoon-24node-mesh-mode4.yaml"
            ;;
        48)
            TOPOLOGY="2_platoons_6_squads"
            TOPO_FILE="topologies/battalion-48node-mesh-mode4.yaml"
            ;;
        96)
            TOPOLOGY="4_platoons_12_squads"
            TOPO_FILE="topologies/battalion-96node-mesh-mode4.yaml"
            ;;
        384)
            TOPOLOGY="multi_company_384n"
            TOPO_FILE=""  # Will be generated
            ;;
        1000)
            TOPOLOGY="battalion_1000n"
            TOPO_FILE=""  # Will be generated
            ;;
    esac

    for BANDWIDTH in "${BANDWIDTHS[@]}"; do
        CURRENT_TEST=$((CURRENT_TEST + 1))
        TEST_NAME="peat-hierarchical-${NODE_COUNT}n-${BANDWIDTH}"

        echo "  [${CURRENT_TEST}/${TOTAL_TESTS}] ${TEST_NAME}"

        # Check if topology exists, otherwise generate it
        if [ "$NODE_COUNT" -le 96 ] && [ -n "$TOPO_FILE" ]; then
            # Use existing pre-generated topologies (same topology for all bandwidths)
            if [ ! -f "$TOPO_FILE" ]; then
                echo "    ⚠️  Topology not found: $TOPO_FILE"
                echo "${NODE_COUNT},${BANDWIDTH},${TOPOLOGY},0,0,0,0,0,0,SKIP" >> "$RESULTS_CSV"
                continue
            fi
        else
            # Generate large-scale topologies on demand
            TOPO_FILE="${RESULTS_DIR}/${TEST_NAME}.yaml"
            echo "    Generating ${NODE_COUNT}-node hierarchical topology..."

            # Use Lab 4 topology generator for large scales
            python3 generate-lab4-hierarchical-topology.py \
                --nodes "${NODE_COUNT}" \
                --bandwidth "${BANDWIDTH}" \
                --output "${TOPO_FILE}" || {
                echo "    ⚠️  Failed to generate topology"
                echo "${NODE_COUNT},${BANDWIDTH},${TOPOLOGY},0,0,0,0,0,0,0,0,FAIL" >> "$RESULTS_CSV"
                continue
            }
        fi

        # Get lab name and count expected nodes from topology file
        LAB_NAME=$(grep '^name:' "$TOPO_FILE" | awk '{print $2}')
        EXPECTED_NODES=$(grep -c "kind: linux" "$TOPO_FILE")

        # Deploy topology
        echo "    Deploying topology (${EXPECTED_NODES} nodes)..."
        if ! containerlab deploy -t "$TOPO_FILE" --reconfigure > /dev/null 2>&1; then
            echo "    ❌ Deployment failed"
            echo "${NODE_COUNT},${BANDWIDTH},${TOPOLOGY},0,0,0,0,0,0,0,0,FAIL" >> "$RESULTS_CSV"
            continue
        fi

        # Wait for all containers to be running
        echo "    Waiting for all ${EXPECTED_NODES} containers to be running..."
        while true; do
            RUNNING_COUNT=$(docker ps --filter "name=clab-${LAB_NAME}" --format "{{.Names}}" 2>/dev/null | wc -l)
            if [ "$RUNNING_COUNT" -ge "$EXPECTED_NODES" ]; then
                echo "    All ${RUNNING_COUNT} containers running"
                break
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
        SOLDIER_CONTAINER=$(docker ps --format '{{.Names}}' | grep "${LAB_NAME}" | grep -E "(soldier|alpha-soldier)" | head -1)
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

        # Extract CROSS-TIER PROPAGATION latencies from DocumentReceived events
        # These measure actual network sync time, not local processing
        SOLDIER_TO_SQUAD_P50=0
        SOLDIER_TO_SQUAD_P95=0
        SQUAD_TO_PLATOON_P50=0
        SQUAD_TO_PLATOON_P95=0
        AGGREGATION_RATIO=0
        TOTAL_OPS=0
        STATUS="PASS"

        # Parse Soldier → Squad aggregation latency
        # Squad leaders aggregate soldier NodeState documents - use CRDTUpsert latency
        # First try DocumentReceived (pre-built topologies), then fall back to CRDTUpsert (generated topologies)
        cat "${LOG_DIR}"/*squad*leader*.log 2>/dev/null | \
            grep 'METRICS:' | \
            grep '"event_type":"DocumentReceived"' | \
            grep -v '"latency_type":"creation"' | \
            grep 'sim_doc_' | \
            sed 's/.*"latency_ms":\([0-9.]*\).*/\1/' | \
            sort -n > /tmp/soldier_to_squad_lat_$$.txt || true

        # Fall back to CRDTUpsert events for generated topologies (384n, 1000n)
        if [ ! -s /tmp/soldier_to_squad_lat_$$.txt ]; then
            cat "${LOG_DIR}"/*squad*leader*.log 2>/dev/null | \
                grep 'METRICS:' | \
                grep '"event_type":"CRDTUpsert"' | \
                grep '"tier":"squad_leader"' | \
                sed 's/.*"latency_ms":\([0-9.]*\).*/\1/' | \
                sort -n > /tmp/soldier_to_squad_lat_$$.txt || true
        fi

        if [ -s /tmp/soldier_to_squad_lat_$$.txt ]; then
            SOLDIER_TO_SQUAD_P50=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) print a[int(c*0.5)]; else print 0}' /tmp/soldier_to_squad_lat_$$.txt)
            SOLDIER_TO_SQUAD_P95=$(awk 'BEGIN{c=0} {a[c++]=$1} END{if(c>0) print a[int(c*0.95)]; else print 0}' /tmp/soldier_to_squad_lat_$$.txt)
        fi
        rm -f /tmp/soldier_to_squad_lat_$$.txt

        # Parse Squad → Platoon propagation latency
        # Platoon leaders receive SquadSummary documents via CRDT sync
        # First try DocumentReceived (pre-built topologies), then fall back to CRDTUpsert (generated topologies)
        cat "${LOG_DIR}"/*platoon*leader*.log 2>/dev/null | \
            grep 'METRICS:' | \
            grep '"event_type":"DocumentReceived"' | \
            grep -v '"latency_type":"creation"' | \
            grep 'squad-.*-summary' | \
            sed 's/.*"latency_ms":\([0-9.]*\).*/\1/' | \
            sort -n > /tmp/squad_to_platoon_lat_$$.txt || true

        # Fall back to CRDTUpsert events from platoon leaders (generated topologies)
        if [ ! -s /tmp/squad_to_platoon_lat_$$.txt ]; then
            cat "${LOG_DIR}"/*platoon*leader*.log 2>/dev/null | \
                grep 'METRICS:' | \
                grep '"event_type":"CRDTUpsert"' | \
                grep '"tier":"platoon_leader"' | \
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
        docker network ls --format '{{.Name}}' | grep "lab4-hierarchical" | xargs -r docker network rm 2>/dev/null || true
        sleep 2
    done
done

PASSED=$(grep -c ",PASS$" "$RESULTS_CSV" || echo "0")
FAILED=$(grep -c ",FAIL$" "$RESULTS_CSV" || echo "0")
WARNED=$(grep -c ",WARN$" "$RESULTS_CSV" || echo "0")

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Lab 4: Hierarchical PEAT CRDT Tests Complete            ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results: ${RESULTS_DIR}"
echo "Passed: ${PASSED} / Warnings: ${WARNED} / Failed: ${FAILED}"
echo ""
echo "Lab 4 tests hierarchical topology WITH PEAT CRDT"
echo "Compare to Lab 3b (flat mesh) to measure hierarchy scaling benefit"
echo ""
echo "Analysis:"
echo "  - Soldier→Squad latency: Cross-tier CRDT propagation from soldier to squad leader"
echo "  - Squad→Platoon latency: Cross-tier CRDT propagation from squad to platoon leader"
echo "  - Aggregation ratio: Documents reduced at each tier"
echo ""
echo "Note: Latencies are NETWORK PROPAGATION times (actual sync delay), not local processing"
echo ""

# Generate comparison if Lab 3b results exist
if ls peat-flat-mesh-*/peat-flat-mesh-results.csv 2>/dev/null | head -1 > /dev/null; then
    LAB3B_RESULTS=$(ls -t peat-flat-mesh-*/peat-flat-mesh-results.csv | head -1)
    echo "Comparison to Lab 3b (flat mesh):"
    echo "  Lab 3b results: ${LAB3B_RESULTS}"
    echo "  Lab 4 results: ${RESULTS_CSV}"
    echo ""
    echo "Run: python3 compare-lab3b-vs-lab4.py ${LAB3B_RESULTS} ${RESULTS_CSV}"
fi
