#!/bin/bash
# Lab 4: Backend Parity Validation Test
#
# Pre-flight check to ensure Ditto and Automerge backends produce comparable
# results before running full-scale experiments.
#
# Issue #519: https://github.com/kitplummer/hive/issues/519
#
# Usage:
#   ./test-lab4-parity.sh                    # Run default 24-node parity test
#   ./test-lab4-parity.sh --nodes 48         # Run with 48 nodes
#   ./test-lab4-parity.sh --duration 60      # Run for 60 seconds
#   ./test-lab4-parity.sh --threshold 20     # Allow 20% variance (default: 15%)
#   ./test-lab4-parity.sh --quick            # Quick 30-second validation
#
# Exit codes:
#   0 - Parity check PASSED (backends produce comparable results)
#   1 - Parity check FAILED (significant variance detected)
#   2 - Infrastructure error (deployment failed, missing tools, etc.)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

source ./test-common.sh

# Configuration
NODES=24
DURATION_SECS=60
BANDWIDTH="1gbps"
VARIANCE_THRESHOLD=15  # Percentage variance allowed between backends
RESULTS_BASE_DIR="/work/hive-sim-results/parity-tests"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --nodes)
            NODES="$2"
            shift 2
            ;;
        --duration)
            DURATION_SECS="$2"
            shift 2
            ;;
        --bandwidth)
            BANDWIDTH="$2"
            shift 2
            ;;
        --threshold)
            VARIANCE_THRESHOLD="$2"
            shift 2
            ;;
        --quick)
            DURATION_SECS=30
            shift
            ;;
        --help|-h)
            echo "Lab 4 Backend Parity Validation"
            echo ""
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --nodes <n>       Node count (default: 24)"
            echo "  --duration <s>    Test duration in seconds (default: 60)"
            echo "  --bandwidth <bw>  Bandwidth constraint (default: 1gbps)"
            echo "  --threshold <p>   Max allowed variance percentage (default: 15)"
            echo "  --quick           Quick 30-second validation"
            echo "  --help            Show this help"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 2
            ;;
    esac
done

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="${RESULTS_BASE_DIR}/parity-${NODES}n-${TIMESTAMP}"

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "  Lab 4: Backend Parity Validation Test (Issue #519)"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "  Configuration:"
echo "    Nodes:     ${NODES}"
echo "    Duration:  ${DURATION_SECS}s"
echo "    Bandwidth: ${BANDWIDTH}"
echo "    Threshold: ${VARIANCE_THRESHOLD}% variance allowed"
echo ""

# Pre-flight checks
log_info "Running pre-flight checks..."

for cmd in docker containerlab python3 bc; do
    if ! command -v $cmd &> /dev/null; then
        log_error "$cmd not found"
        exit 2
    fi
done

# Check for required Docker images
for tag in ditto automerge; do
    if ! docker images | grep -q "hive-sim.*${tag}"; then
        log_warning "hive-sim:${tag} image not found, will try hive-sim-node:latest"
    fi
done

# Load credentials
if [ -f "../.env" ]; then
    set -a
    source ../.env
    set +a
    log_info "Loaded credentials from .env"
fi

log_success "Pre-flight checks passed"

# Create results directory
mkdir -p "${RESULTS_DIR}"/{ditto,automerge}

# Generate topology (backend-agnostic)
TOPO_FILE="${RESULTS_DIR}/topology.yaml"
log_info "Generating ${NODES}-node topology..."

python3 generate-lab4-hierarchical-topology.py \
    --nodes "${NODES}" \
    --bandwidth "${BANDWIDTH}" \
    --output "${TOPO_FILE}" || {
    log_error "Failed to generate topology"
    exit 2
}

LAB_NAME=$(grep '^name:' "$TOPO_FILE" | awk '{print $2}')
EXPECTED_NODES=$(grep -c "kind: linux" "$TOPO_FILE")
log_success "Generated topology: ${LAB_NAME} (${EXPECTED_NODES} nodes)"

# Cleanup any existing deployment
log_info "Cleaning up existing containers..."
docker ps -aq --filter "name=clab-${LAB_NAME}" 2>/dev/null | xargs -r docker rm -f > /dev/null 2>&1 || true
docker network ls --format '{{.Name}}' | grep "${LAB_NAME}" | xargs -r docker network rm 2>/dev/null || true

# Run backend test function
run_backend_test() {
    local BACKEND="$1"
    local BACKEND_RESULTS="${RESULTS_DIR}/${BACKEND}"

    log_info "Testing ${BACKEND} backend..."

    # Set up credentials
    if [[ "$BACKEND" == "automerge" ]]; then
        export HIVE_APP_ID="test-formation"
        export HIVE_SECRET_KEY="aGl2ZS10ZXN0LWZvcm1hdGlvbi1zZWNyZXQta2V5LTA="
        export HIVE_OFFLINE_TOKEN=""
        export HIVE_SHARED_KEY=""
    else
        if [ -f "../.env" ]; then
            set -a
            source ../.env
            set +a
        fi
        export HIVE_SECRET_KEY=""
    fi

    # Substitute backend in topology
    export BACKEND
    local TOPO_RESOLVED="${BACKEND_RESULTS}/topology-resolved.yaml"
    envsubst < "$TOPO_FILE" > "$TOPO_RESOLVED"

    # Deploy
    local DEPLOY_START=$(date +%s)
    if ! containerlab deploy -t "$TOPO_RESOLVED" --reconfigure --timeout 10m > "${BACKEND_RESULTS}/deploy.log" 2>&1; then
        log_error "${BACKEND} deployment failed"
        tail -10 "${BACKEND_RESULTS}/deploy.log"
        return 1
    fi
    local DEPLOY_END=$(date +%s)
    local DEPLOY_TIME=$((DEPLOY_END - DEPLOY_START))

    # Verify all containers running
    local RUNNING=$(docker ps --filter "name=clab-${LAB_NAME}" --filter "status=running" -q 2>/dev/null | wc -l)
    if [[ "$RUNNING" -lt "$EXPECTED_NODES" ]]; then
        log_error "${BACKEND}: Only ${RUNNING}/${EXPECTED_NODES} containers running"
        return 1
    fi

    log_success "${BACKEND}: Deployed ${RUNNING} nodes in ${DEPLOY_TIME}s"

    # Run test
    log_info "${BACKEND}: Running test for ${DURATION_SECS}s..."
    sleep "${DURATION_SECS}"

    # Collect metrics from leader nodes
    log_info "${BACKEND}: Collecting metrics..."

    local METRICS_FILE="${BACKEND_RESULTS}/metrics.json"
    echo "{\"backend\": \"${BACKEND}\", \"nodes\": ${NODES}, \"duration\": ${DURATION_SECS}}" > "$METRICS_FILE"

    # Collect from squad leaders
    local SQUAD_LEADERS=$(docker ps --format '{{.Names}}' | grep "clab-${LAB_NAME}" | grep "squad.*leader" || true)
    local SQUAD_OPS=0

    if [ -n "$SQUAD_LEADERS" ]; then
        for CONTAINER in $SQUAD_LEADERS; do
            local LOG="${BACKEND_RESULTS}/$(echo $CONTAINER | sed "s/clab-${LAB_NAME}-//").log"
            docker logs "$CONTAINER" 2>&1 > "$LOG" || true

            # Count CRDT operations
            local OPS=$(grep -c 'METRICS:.*CRDTUpsert' "$LOG" 2>/dev/null || echo "0")
            SQUAD_OPS=$((SQUAD_OPS + ${OPS:-0}))

            # Extract latencies
            grep 'METRICS:' "$LOG" 2>/dev/null | \
                grep '"event_type":"CRDTUpsert"' | \
                sed 's/.*"latency_ms":\([0-9.]*\).*/\1/' >> "${BACKEND_RESULTS}/latencies.txt" 2>/dev/null || true
        done
    fi

    # Collect from platoon leaders
    local PLATOON_LEADERS=$(docker ps --format '{{.Names}}' | grep "clab-${LAB_NAME}" | grep "platoon.*leader" || true)
    local PLATOON_OPS=0

    if [ -n "$PLATOON_LEADERS" ]; then
        for CONTAINER in $PLATOON_LEADERS; do
            local LOG="${BACKEND_RESULTS}/$(echo $CONTAINER | sed "s/clab-${LAB_NAME}-//").log"
            docker logs "$CONTAINER" 2>&1 > "$LOG" || true

            local OPS=$(grep -c 'METRICS:.*CRDTUpsert' "$LOG" 2>/dev/null || echo "0")
            PLATOON_OPS=$((PLATOON_OPS + ${OPS:-0}))

            grep 'METRICS:' "$LOG" 2>/dev/null | \
                grep '"event_type":"CRDTUpsert"' | \
                sed 's/.*"latency_ms":\([0-9.]*\).*/\1/' >> "${BACKEND_RESULTS}/latencies.txt" 2>/dev/null || true
        done
    fi

    # Calculate aggregation events (SquadSummaryCreated, PlatoonSummaryCreated)
    local SQUAD_SUMMARIES=0
    local PLATOON_SUMMARIES=0

    if ls "${BACKEND_RESULTS}"/*.log >/dev/null 2>&1; then
        SQUAD_SUMMARIES=$(grep -h 'SquadSummaryCreated' "${BACKEND_RESULTS}"/*.log 2>/dev/null | wc -l)
        PLATOON_SUMMARIES=$(grep -h 'PlatoonSummaryCreated' "${BACKEND_RESULTS}"/*.log 2>/dev/null | wc -l)
    fi

    # Calculate latency percentiles
    local LAT_FILE="${BACKEND_RESULTS}/latencies.txt"
    local P50=0 P95=0 P99=0

    if [ -s "$LAT_FILE" ]; then
        P50=$(sort -n "$LAT_FILE" | awk 'BEGIN{c=0} {a[c++]=$1} END{print a[int(c*0.5)]}')
        P95=$(sort -n "$LAT_FILE" | awk 'BEGIN{c=0} {a[c++]=$1} END{print a[int(c*0.95)]}')
        P99=$(sort -n "$LAT_FILE" | awk 'BEGIN{c=0} {a[c++]=$1} END{print a[int(c*0.99)]}')
    fi

    # Write summary
    cat > "${BACKEND_RESULTS}/summary.txt" << EOF
backend=${BACKEND}
total_crdt_ops=$((SQUAD_OPS + PLATOON_OPS))
squad_ops=${SQUAD_OPS}
platoon_ops=${PLATOON_OPS}
squad_summaries_created=${SQUAD_SUMMARIES}
platoon_summaries_created=${PLATOON_SUMMARIES}
latency_p50_ms=${P50}
latency_p95_ms=${P95}
latency_p99_ms=${P99}
deploy_time_secs=${DEPLOY_TIME}
EOF

    log_success "${BACKEND}: Collected metrics (${SQUAD_OPS} squad ops, ${PLATOON_OPS} platoon ops)"
    return 0
}

# Run both backends
DITTO_STATUS=0
AUTOMERGE_STATUS=0

# Test Ditto
if ! run_backend_test "ditto"; then
    DITTO_STATUS=1
fi

# Test Automerge (reconfigure existing deployment)
if ! run_backend_test "automerge"; then
    AUTOMERGE_STATUS=1
fi

# Cleanup
log_info "Cleaning up..."
docker ps -aq --filter "name=clab-${LAB_NAME}" 2>/dev/null | xargs -r docker rm -f > /dev/null 2>&1 || true
docker network ls --format '{{.Name}}' | grep "${LAB_NAME}" | xargs -r docker network rm 2>/dev/null || true

# Check if both backends ran successfully
if [[ "$DITTO_STATUS" -ne 0 || "$AUTOMERGE_STATUS" -ne 0 ]]; then
    log_error "One or both backends failed to run"
    exit 2
fi

# Compare results
echo ""
echo "════════════════════════════════════════════════════════════════"
echo "  Parity Comparison"
echo "════════════════════════════════════════════════════════════════"
echo ""

# Load summaries
source "${RESULTS_DIR}/ditto/summary.txt"
DITTO_OPS=$total_crdt_ops
DITTO_SQUAD_SUM=$squad_summaries_created
DITTO_PLATOON_SUM=$platoon_summaries_created
DITTO_P50=$latency_p50_ms
DITTO_P95=$latency_p95_ms

source "${RESULTS_DIR}/automerge/summary.txt"
AM_OPS=$total_crdt_ops
AM_SQUAD_SUM=$squad_summaries_created
AM_PLATOON_SUM=$platoon_summaries_created
AM_P50=$latency_p50_ms
AM_P95=$latency_p95_ms

# Calculate variance helper
calc_variance() {
    local A="$1"
    local B="$2"

    if [[ "$A" -eq 0 && "$B" -eq 0 ]]; then
        echo "0"
        return
    fi

    local MAX=$(( A > B ? A : B ))
    local MIN=$(( A < B ? A : B ))

    if [[ "$MAX" -eq 0 ]]; then
        echo "0"
        return
    fi

    echo "scale=1; (($MAX - $MIN) * 100) / $MAX" | bc
}

# Calculate variances
OPS_VARIANCE=$(calc_variance "$DITTO_OPS" "$AM_OPS")
SQUAD_SUM_VARIANCE=$(calc_variance "$DITTO_SQUAD_SUM" "$AM_SQUAD_SUM")
PLATOON_SUM_VARIANCE=$(calc_variance "$DITTO_PLATOON_SUM" "$AM_PLATOON_SUM")

# Print comparison table
printf "%-30s %15s %15s %10s\n" "Metric" "Ditto" "Automerge" "Variance"
printf "%-30s %15s %15s %10s\n" "------------------------------" "---------------" "---------------" "----------"
printf "%-30s %15d %15d %9.1f%%\n" "Total CRDT Operations" "$DITTO_OPS" "$AM_OPS" "$OPS_VARIANCE"
printf "%-30s %15d %15d %9.1f%%\n" "Squad Summaries Created" "$DITTO_SQUAD_SUM" "$AM_SQUAD_SUM" "$SQUAD_SUM_VARIANCE"
printf "%-30s %15d %15d %9.1f%%\n" "Platoon Summaries Created" "$DITTO_PLATOON_SUM" "$AM_PLATOON_SUM" "$PLATOON_SUM_VARIANCE"
printf "%-30s %15.2f %15.2f %10s\n" "Latency P50 (ms)" "$DITTO_P50" "$AM_P50" "N/A"
printf "%-30s %15.2f %15.2f %10s\n" "Latency P95 (ms)" "$DITTO_P95" "$AM_P95" "N/A"
echo ""

# Check ADR-021 compliance (create-once pattern)
echo "ADR-021 Compliance Check:"

# Expected squad summaries = number of squads (roughly nodes/8 for 7 soldiers + 1 leader)
EXPECTED_SQUADS=$((NODES / 8))
if [[ "$EXPECTED_SQUADS" -lt 1 ]]; then
    EXPECTED_SQUADS=1
fi

ADR021_DITTO="PASS"
ADR021_AUTOMERGE="PASS"

if [[ "$DITTO_SQUAD_SUM" -gt $((EXPECTED_SQUADS * 3)) ]]; then
    ADR021_DITTO="WARN"
    log_warning "Ditto: ${DITTO_SQUAD_SUM} squad summaries created (expected ~${EXPECTED_SQUADS})"
fi

if [[ "$AM_SQUAD_SUM" -gt $((EXPECTED_SQUADS * 3)) ]]; then
    ADR021_AUTOMERGE="WARN"
    log_warning "Automerge: ${AM_SQUAD_SUM} squad summaries created (expected ~${EXPECTED_SQUADS})"
fi

printf "  Ditto:     %s (created %d summaries, expected ~%d)\n" "$ADR021_DITTO" "$DITTO_SQUAD_SUM" "$EXPECTED_SQUADS"
printf "  Automerge: %s (created %d summaries, expected ~%d)\n" "$ADR021_AUTOMERGE" "$AM_SQUAD_SUM" "$EXPECTED_SQUADS"
echo ""

# Determine parity status
PARITY_PASSED=true
FAILURES=""

# Check operation count variance
if (( $(echo "$OPS_VARIANCE > $VARIANCE_THRESHOLD" | bc -l) )); then
    PARITY_PASSED=false
    FAILURES="${FAILURES}\n  - Operation count variance ${OPS_VARIANCE}% exceeds threshold ${VARIANCE_THRESHOLD}%"
fi

# Check aggregation event variance
if (( $(echo "$SQUAD_SUM_VARIANCE > $VARIANCE_THRESHOLD" | bc -l) )); then
    PARITY_PASSED=false
    FAILURES="${FAILURES}\n  - Squad summary variance ${SQUAD_SUM_VARIANCE}% exceeds threshold ${VARIANCE_THRESHOLD}%"
fi

# Save parity report
cat > "${RESULTS_DIR}/parity-report.txt" << EOF
Lab 4 Backend Parity Report
===========================

Test Configuration:
  Nodes:     ${NODES}
  Duration:  ${DURATION_SECS}s
  Bandwidth: ${BANDWIDTH}
  Threshold: ${VARIANCE_THRESHOLD}%

Results:
  Ditto Total Ops:     ${DITTO_OPS}
  Automerge Total Ops: ${AM_OPS}
  Operation Variance:  ${OPS_VARIANCE}%

  Ditto Squad Summaries:     ${DITTO_SQUAD_SUM}
  Automerge Squad Summaries: ${AM_SQUAD_SUM}
  Summary Variance:          ${SQUAD_SUM_VARIANCE}%

  Ditto P50 Latency:     ${DITTO_P50}ms
  Automerge P50 Latency: ${AM_P50}ms

  Ditto P95 Latency:     ${DITTO_P95}ms
  Automerge P95 Latency: ${AM_P95}ms

ADR-021 Compliance:
  Ditto:     ${ADR021_DITTO}
  Automerge: ${ADR021_AUTOMERGE}

Parity Status: $(if $PARITY_PASSED; then echo "PASSED"; else echo "FAILED"; fi)
$(if ! $PARITY_PASSED; then echo -e "Failures:${FAILURES}"; fi)

Generated: $(date)
EOF

echo ""
echo "════════════════════════════════════════════════════════════════"

if $PARITY_PASSED; then
    echo ""
    log_success "PARITY CHECK PASSED"
    echo ""
    echo "  Backends produce comparable results within ${VARIANCE_THRESHOLD}% variance."
    echo "  Safe to proceed with Lab 4 full-scale experiments."
    echo ""
    echo "  Report: ${RESULTS_DIR}/parity-report.txt"
    echo ""
    exit 0
else
    echo ""
    log_error "PARITY CHECK FAILED"
    echo ""
    echo -e "  Issues detected:${FAILURES}"
    echo ""
    echo "  Review results in: ${RESULTS_DIR}"
    echo "  Consider investigating before running large-scale experiments."
    echo ""
    exit 1
fi
