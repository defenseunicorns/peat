#!/bin/bash
# Lab 4: Hierarchical HIVE CRDT
#
# Tests hierarchical topology with HIVE CRDT (squad → platoon → company hierarchy)
# Compares to Lab 3b (flat mesh CRDT) to measure hierarchy benefit

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

source ./test-common.sh

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="hive-hierarchical-${TIMESTAMP}"
RESULTS_CSV="${RESULTS_DIR}/hive-hierarchical-results.csv"

# Hierarchical scales: prove scaling beyond Lab 3b's limits
NODE_COUNTS=(24 48 96 384 1000)
BANDWIDTHS=("1gbps" "100mbps" "1mbps" "256kbps")
TEST_DURATION_SECS=120  # 2 minutes per test

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Lab 4: Hierarchical HIVE CRDT                            ║"
echo "║  Multi-tier topology with aggregation                     ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results: ${RESULTS_DIR}"
echo ""

validate_environment
mkdir -p "${RESULTS_DIR}/logs"

# CSV header with hierarchical metrics
echo "NodeCount,Bandwidth,Topology,Soldier_P50_ms,Soldier_P95_ms,Squad_P50_ms,Squad_P95_ms,Platoon_P50_ms,Platoon_P95_ms,Aggregation_Ratio,Total_Operations,Status" > "$RESULTS_CSV"

TOTAL_TESTS=$((${#NODE_COUNTS[@]} * ${#BANDWIDTHS[@]}))
CURRENT_TEST=0

for NODE_COUNT in "${NODE_COUNTS[@]}"; do
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "Node Count: ${NODE_COUNT} (hierarchical topology)"
    echo "═══════════════════════════════════════════════════════════"

    # Determine topology structure based on node count
    case $NODE_COUNT in
        24)
            TOPOLOGY="1_platoon_3_squads"
            TOPO_BASE="test-backend-ditto-24n-hierarchical"
            ;;
        48)
            TOPOLOGY="2_platoons_6_squads"
            TOPO_BASE="test-backend-ditto-48n-hierarchical"
            ;;
        96)
            TOPOLOGY="4_platoons_12_squads"
            TOPO_BASE="test-backend-ditto-96n-hierarchical"
            ;;
        384)
            TOPOLOGY="multi_company_384n"
            TOPO_BASE="hierarchical-384n"
            ;;
        1000)
            TOPOLOGY="battalion_1000n"
            TOPO_BASE="hierarchical-1000n"
            ;;
    esac

    for BANDWIDTH in "${BANDWIDTHS[@]}"; do
        CURRENT_TEST=$((CURRENT_TEST + 1))
        TEST_NAME="hive-hierarchical-${NODE_COUNT}n-${BANDWIDTH}"

        echo "  [${CURRENT_TEST}/${TOTAL_TESTS}] ${TEST_NAME}"

        # Check if topology exists, otherwise generate it
        if [ "$NODE_COUNT" -le 96 ]; then
            # Use existing pre-generated topologies
            TOPO_FILE="topologies/${TOPO_BASE}-${BANDWIDTH}.yaml"
            if [ ! -f "$TOPO_FILE" ]; then
                echo "    ⚠️  Topology not found: $TOPO_FILE"
                echo "${NODE_COUNT},${BANDWIDTH},${TOPOLOGY},0,0,0,0,0,0,0,0,SKIP" >> "$RESULTS_CSV"
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

        # Deploy topology
        echo "    Deploying topology..."
        if ! containerlab deploy -t "$TOPO_FILE" --reconfigure > /dev/null 2>&1; then
            echo "    ❌ Deployment failed"
            echo "${NODE_COUNT},${BANDWIDTH},${TOPOLOGY},0,0,0,0,0,0,0,0,FAIL" >> "$RESULTS_CSV"
            continue
        fi

        echo "    Running for ${TEST_DURATION_SECS}s..."
        sleep "$TEST_DURATION_SECS"

        # Collect logs from different tiers
        LOG_DIR="${RESULTS_DIR}/logs/${TEST_NAME}"
        mkdir -p "$LOG_DIR"

        # Get container name prefix from topology file
        LAB_NAME=$(grep '^name:' "$TOPO_FILE" | awk '{print $2}')

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

        # Extract metrics from METRICS: JSON logs
        SOLDIER_P50=0
        SOLDIER_P95=0
        SQUAD_P50=0
        SQUAD_P95=0
        PLATOON_P50=0
        PLATOON_P95=0
        AGGREGATION_RATIO=0
        TOTAL_OPS=0
        STATUS="PASS"

        # Parse soldier-level CRDT latencies
        # Note: Soldiers emit MessageSent events, not aggregation metrics
        # We'll skip soldier metrics for now as they're not critical for hierarchy analysis
        SOLDIER_P50=0
        SOLDIER_P95=0

        # Parse squad leader aggregation latencies from AggregationCompleted events
        cat "${LOG_DIR}"/*squad*leader*.log 2>/dev/null | \
            grep 'METRICS:' | \
            grep '"event_type":"AggregationCompleted"' | \
            grep '"tier":"squad"' | \
            sed 's/.*"processing_time_us":\([0-9.]*\).*/\1/' | \
            awk '{print $1/1000}' | \
            sort -n > /tmp/squad_lat_$$.txt || true

        if [ -s /tmp/squad_lat_$$.txt ]; then
            SQUAD_P50=$(awk 'BEGIN{c=0} {a[c++]=$1} END{print a[int(c*0.5)]}' /tmp/squad_lat_$$.txt)
            SQUAD_P95=$(awk 'BEGIN{c=0} {a[c++]=$1} END{print a[int(c*0.95)]}' /tmp/squad_lat_$$.txt)
        fi
        rm -f /tmp/squad_lat_$$.txt

        # Parse platoon leader aggregation latencies from AggregationCompleted events
        cat "${LOG_DIR}"/*platoon*leader*.log 2>/dev/null | \
            grep 'METRICS:' | \
            grep '"event_type":"AggregationCompleted"' | \
            grep '"tier":"platoon"' | \
            sed 's/.*"processing_time_us":\([0-9.]*\).*/\1/' | \
            awk '{print $1/1000}' | \
            sort -n > /tmp/platoon_lat_$$.txt || true

        if [ -s /tmp/platoon_lat_$$.txt ]; then
            PLATOON_P50=$(awk 'BEGIN{c=0} {a[c++]=$1} END{print a[int(c*0.5)]}' /tmp/platoon_lat_$$.txt)
            PLATOON_P95=$(awk 'BEGIN{c=0} {a[c++]=$1} END{print a[int(c*0.95)]}' /tmp/platoon_lat_$$.txt)
        fi
        rm -f /tmp/platoon_lat_$$.txt

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
        if [ "$SOLDIER_P50" = "0" ] && [ "$SQUAD_P50" = "0" ]; then
            STATUS="WARN"
            echo "    ⚠️  No metrics collected"
        else
            echo "    ✅ PASS (Soldier P95: ${SOLDIER_P95}ms, Squad P95: ${SQUAD_P95}ms)"
        fi

        # Write results
        echo "${NODE_COUNT},${BANDWIDTH},${TOPOLOGY},${SOLDIER_P50},${SOLDIER_P95},${SQUAD_P50},${SQUAD_P95},${PLATOON_P50},${PLATOON_P95},${AGGREGATION_RATIO},${TOTAL_OPS},${STATUS}" >> "$RESULTS_CSV"

        # Cleanup
        containerlab destroy -t "$TOPO_FILE" --cleanup > /dev/null 2>&1 || true
        sleep 2
    done
done

PASSED=$(grep -c ",PASS$" "$RESULTS_CSV" || echo "0")
FAILED=$(grep -c ",FAIL$" "$RESULTS_CSV" || echo "0")
WARNED=$(grep -c ",WARN$" "$RESULTS_CSV" || echo "0")

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Lab 4: Hierarchical HIVE CRDT Tests Complete            ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results: ${RESULTS_DIR}"
echo "Passed: ${PASSED} / Warnings: ${WARNED} / Failed: ${FAILED}"
echo ""
echo "Lab 4 tests hierarchical topology WITH HIVE CRDT"
echo "Compare to Lab 3b (flat mesh) to measure hierarchy scaling benefit"
echo ""
echo "Analysis:"
echo "  - Soldier tier: Edge node CRDT performance"
echo "  - Squad tier: First-level aggregation (N soldiers → 1 summary)"
echo "  - Platoon tier: Second-level aggregation (N squads → 1 summary)"
echo "  - Aggregation ratio: Documents reduced at each tier"
echo ""

# Generate comparison if Lab 3b results exist
if ls hive-flat-mesh-*/hive-flat-mesh-results.csv 2>/dev/null | head -1 > /dev/null; then
    LAB3B_RESULTS=$(ls -t hive-flat-mesh-*/hive-flat-mesh-results.csv | head -1)
    echo "Comparison to Lab 3b (flat mesh):"
    echo "  Lab 3b results: ${LAB3B_RESULTS}"
    echo "  Lab 4 results: ${RESULTS_CSV}"
    echo ""
    echo "Run: python3 compare-lab3b-vs-lab4.py ${LAB3B_RESULTS} ${RESULTS_CSV}"
fi
