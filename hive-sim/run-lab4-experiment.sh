#!/bin/bash
# run-lab4-experiment.sh - Robust Lab 4 experiment runner
#
# Key features:
# - Pre-flight memory check (fails fast if insufficient)
# - Streaming log collection (logs persist even on OOM)
# - Watchdog for memory pressure
# - Robust cleanup on any exit
#
# Usage:
#   ./run-lab4-experiment.sh [--nodes 384] [--backend automerge] [--bandwidth 1gbps] [--duration 120]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Source .env for credentials (DITTO_*, HIVE_* env vars)
if [[ -f "../.env" ]]; then
    set -a
    source "../.env"
    set +a
    echo "Loaded credentials from ../.env"
else
    echo "WARNING: ../.env not found - Ditto backend will fail without credentials"
fi

# Defaults
NODE_COUNT=384
BACKEND="automerge"
BANDWIDTH="1gbps"
TEST_DURATION_SECS=120
RESULTS_BASE="/work/hive-sim-results"

# Memory requirements (MB per node, empirically determined)
# Automerge+Iroh uses more memory than Ditto
declare -A MEM_PER_NODE=(
    ["automerge"]=150
    ["ditto"]=120
)

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --nodes) NODE_COUNT="$2"; shift 2 ;;
        --backend) BACKEND="$2"; shift 2 ;;
        --bandwidth) BANDWIDTH="$2"; shift 2 ;;
        --duration) TEST_DURATION_SECS="$2"; shift 2 ;;
        --results-dir) RESULTS_BASE="$2"; shift 2 ;;
        -h|--help)
            echo "Usage: $0 [--nodes N] [--backend automerge|ditto] [--bandwidth 1gbps|100mbps|1mbps|256kbps] [--duration SECS]"
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
LAB_NAME="lab4-${BACKEND}-${NODE_COUNT}n-${BANDWIDTH}"
RESULTS_DIR="${RESULTS_BASE}/${LAB_NAME}-${TIMESTAMP}"
TOPO_FILE="${RESULTS_DIR}/${LAB_NAME}.yaml"
LOG_DIR="${RESULTS_DIR}/logs"
STATE_FILE="${RESULTS_DIR}/state.json"

# PIDs to track for cleanup
LOG_COLLECTOR_PID=""
MEMORY_WATCHDOG_PID=""

# ============================================================================
# CLEANUP - runs on ANY exit (normal, error, signal, OOM)
# ============================================================================
cleanup() {
    local exit_code=$?
    echo ""
    echo "========================================"
    echo "CLEANUP (exit code: $exit_code)"
    echo "========================================"

    # Stop background processes
    if [[ -n "$LOG_COLLECTOR_PID" ]] && kill -0 "$LOG_COLLECTOR_PID" 2>/dev/null; then
        echo "Stopping log collector..."
        kill "$LOG_COLLECTOR_PID" 2>/dev/null || true
        wait "$LOG_COLLECTOR_PID" 2>/dev/null || true
    fi

    if [[ -n "$MEMORY_WATCHDOG_PID" ]] && kill -0 "$MEMORY_WATCHDOG_PID" 2>/dev/null; then
        echo "Stopping memory watchdog..."
        kill "$MEMORY_WATCHDOG_PID" 2>/dev/null || true
        wait "$MEMORY_WATCHDOG_PID" 2>/dev/null || true
    fi

    # Final log collection before destroying containers
    if [[ -d "$LOG_DIR" ]]; then
        echo "Final log collection..."
        collect_logs_once "final" || true
    fi

    # Update state
    update_state "phase" "cleanup"
    update_state "exit_code" "$exit_code"
    update_state "cleanup_time" "$(date -Iseconds)"

    # Destroy topology
    if [[ -f "$TOPO_FILE" ]]; then
        echo "Destroying topology..."
        timeout 60 containerlab destroy -t "$TOPO_FILE" --cleanup 2>/dev/null || {
            echo "Graceful destroy failed, forcing..."
            docker ps -aq --filter "name=clab-${CLAB_NAME:-$LAB_NAME}" | xargs -r docker rm -f 2>/dev/null || true
        }
    else
        # No topo file, try to clean by pattern
        docker ps -aq --filter "name=clab-lab4-" | xargs -r docker rm -f 2>/dev/null || true
    fi

    # Prune networks
    docker network ls --format '{{.Name}}' | grep -E "^(lab4-|clab-)" | xargs -r docker network rm 2>/dev/null || true

    update_state "cleanup_complete" "true"

    echo ""
    echo "Results preserved in: $RESULTS_DIR"
    echo "  - Logs: $LOG_DIR"
    echo "  - State: $STATE_FILE"
    echo ""

    # Run analysis automatically
    if [[ -x "${SCRIPT_DIR}/process-lab4-metrics.sh" ]]; then
        echo "========================================"
        echo "Running analysis..."
        echo "========================================"
        "${SCRIPT_DIR}/process-lab4-metrics.sh" "$RESULTS_DIR" || {
            echo "WARNING: Analysis failed (exit code: $?)"
            echo "You can retry manually: ./process-lab4-metrics.sh $RESULTS_DIR"
        }
    else
        echo "To process results:"
        echo "  ./process-lab4-metrics.sh $RESULTS_DIR"
    fi

    exit $exit_code
}

trap cleanup EXIT INT TERM

# ============================================================================
# HELPER FUNCTIONS
# ============================================================================

update_state() {
    local key="$1"
    local value="$2"

    if [[ -f "$STATE_FILE" ]]; then
        # Update existing state file
        local tmp=$(mktemp)
        jq --arg k "$key" --arg v "$value" '.[$k] = $v' "$STATE_FILE" > "$tmp" && mv "$tmp" "$STATE_FILE"
    else
        # Create new state file
        echo "{\"$key\": \"$value\"}" > "$STATE_FILE"
    fi
}

get_available_memory_mb() {
    # Returns available memory in MB
    free -m | awk '/^Mem:/ {print $7}'
}

get_memory_pressure() {
    # Returns percentage of memory used (0-100)
    free | awk '/^Mem:/ {printf "%.0f", $3/$2 * 100}'
}

collect_logs_once() {
    local tag="${1:-snapshot}"
    local snapshot_dir="${LOG_DIR}/${tag}-$(date +%H%M%S)"
    mkdir -p "$snapshot_dir"

    # Get all containers for this lab
    local containers
    containers=$(docker ps --format '{{.Names}}' --filter "name=clab-${CLAB_NAME:-$LAB_NAME}" 2>/dev/null || true)

    if [[ -z "$containers" ]]; then
        return 0
    fi

    # Collect logs from each container (with timeout to prevent hanging)
    echo "$containers" | while read -r container; do
        if [[ -n "$container" ]]; then
            timeout 5 docker logs "$container" 2>&1 > "${snapshot_dir}/${container}.log" 2>/dev/null || true
        fi
    done

    # Count collected logs
    local log_count
    log_count=$(find "$snapshot_dir" -name "*.log" -size +0 | wc -l)
    echo "Collected $log_count logs to $snapshot_dir"
}

start_log_collector() {
    # Background process that collects logs every 30 seconds
    (
        while true; do
            sleep 30
            collect_logs_once "stream" 2>/dev/null || true
        done
    ) &
    LOG_COLLECTOR_PID=$!
    echo "Log collector started (PID: $LOG_COLLECTOR_PID)"
}

start_memory_watchdog() {
    local threshold=90  # Trigger warning at 90% memory usage

    (
        while true; do
            sleep 10
            local pressure
            pressure=$(get_memory_pressure)

            if [[ $pressure -gt $threshold ]]; then
                echo "WARNING: Memory pressure at ${pressure}% - collecting emergency snapshot"
                collect_logs_once "emergency" 2>/dev/null || true
                update_state "memory_warning" "$(date -Iseconds): ${pressure}%"
            fi

            # Log memory status periodically
            echo "$(date +%H:%M:%S) Memory: ${pressure}% used" >> "${RESULTS_DIR}/memory.log"
        done
    ) &
    MEMORY_WATCHDOG_PID=$!
    echo "Memory watchdog started (PID: $MEMORY_WATCHDOG_PID)"
}

# ============================================================================
# MAIN EXECUTION
# ============================================================================

echo "========================================"
echo "Lab 4 Experiment Runner"
echo "========================================"
echo "Nodes:     $NODE_COUNT"
echo "Backend:   $BACKEND"
echo "Bandwidth: $BANDWIDTH"
echo "Duration:  ${TEST_DURATION_SECS}s"
echo "Results:   $RESULTS_DIR"
echo "========================================"
echo ""

# Create results directory
mkdir -p "$LOG_DIR"

# Initialize state
cat > "$STATE_FILE" << EOF
{
    "lab_name": "$LAB_NAME",
    "node_count": $NODE_COUNT,
    "backend": "$BACKEND",
    "bandwidth": "$BANDWIDTH",
    "duration_secs": $TEST_DURATION_SECS,
    "start_time": "$(date -Iseconds)",
    "phase": "preflight"
}
EOF

# ============================================================================
# PHASE 1: PRE-FLIGHT CHECKS
# ============================================================================
echo "Phase 1: Pre-flight checks"
echo "----------------------------------------"

# Check available memory
AVAILABLE_MB=$(get_available_memory_mb)
REQUIRED_MB=$((NODE_COUNT * ${MEM_PER_NODE[$BACKEND]:-150} + 4096))  # +4GB buffer

echo "Available memory: ${AVAILABLE_MB} MB"
echo "Required memory:  ${REQUIRED_MB} MB (${NODE_COUNT} nodes * ${MEM_PER_NODE[$BACKEND]:-150} MB + 4GB buffer)"

if [[ $AVAILABLE_MB -lt $REQUIRED_MB ]]; then
    echo ""
    echo "ERROR: Insufficient memory!"
    echo "  Available: ${AVAILABLE_MB} MB"
    echo "  Required:  ${REQUIRED_MB} MB"
    echo ""
    echo "Options:"
    echo "  1. Free up memory and retry"
    echo "  2. Run with fewer nodes: --nodes $((AVAILABLE_MB / ${MEM_PER_NODE[$BACKEND]:-150} - 30))"
    echo "  3. Use a backend with lower memory requirements"
    update_state "phase" "failed_preflight"
    update_state "error" "insufficient_memory"
    exit 1
fi

echo "Memory check: PASSED"

# Check Docker
if ! docker info &>/dev/null; then
    echo "ERROR: Docker not available"
    exit 1
fi
echo "Docker: OK"

# Check containerlab
if ! command -v containerlab &>/dev/null; then
    echo "ERROR: containerlab not found"
    exit 1
fi
echo "Containerlab: OK"

# Check image exists
IMAGE_TAG="${BACKEND}"
if [[ "$BACKEND" == "automerge" ]]; then
    # Check for hive-sim:automerge image
    if ! docker images "hive-sim:automerge" --format "{{.Repository}}" | grep -q "hive-sim"; then
        echo "ERROR: hive-sim:automerge image not found"
        echo "Build with: RUSTFLAGS=\"\" docker build -f hive-sim/Dockerfile -t hive-sim:automerge --build-arg BACKEND=automerge ."
        exit 1
    fi
fi
echo "Image: OK (hive-sim:${IMAGE_TAG})"

# Clean up any stale resources from previous runs
echo "Cleaning stale resources..."
docker ps -aq --filter "name=clab-lab4-" | xargs -r docker rm -f 2>/dev/null || true
docker network ls --format '{{.Name}}' | grep -E "^lab4-" | xargs -r docker network rm 2>/dev/null || true

update_state "phase" "preflight_complete"
echo "Pre-flight: PASSED"
echo ""

# ============================================================================
# PHASE 2: GENERATE TOPOLOGY
# ============================================================================
echo "Phase 2: Generate topology"
echo "----------------------------------------"

update_state "phase" "generating_topology"

python3 generate-lab4-hierarchical-topology.py \
    --nodes "$NODE_COUNT" \
    --bandwidth "$BANDWIDTH" \
    --backend "$BACKEND" \
    --output "$TOPO_FILE"

if [[ ! -f "$TOPO_FILE" ]]; then
    echo "ERROR: Topology generation failed"
    exit 1
fi

# Expand environment variables in the topology file
# containerlab doesn't expand ${VAR} references, so we do it here
echo "Expanding environment variables in topology..."
envsubst < "$TOPO_FILE" > "${TOPO_FILE}.tmp" && mv "${TOPO_FILE}.tmp" "$TOPO_FILE"

# Extract the actual containerlab name from the YAML (may differ from LAB_NAME)
CLAB_NAME=$(grep "^name:" "$TOPO_FILE" | awk '{print $2}')
echo "Containerlab name: $CLAB_NAME"

EXPECTED_CONTAINERS=$(grep -c "kind: linux" "$TOPO_FILE")
echo "Topology generated: $TOPO_FILE"
echo "Expected containers: $EXPECTED_CONTAINERS"
update_state "expected_containers" "$EXPECTED_CONTAINERS"
update_state "topo_file" "$TOPO_FILE"
echo ""

# ============================================================================
# PHASE 3: DEPLOY
# ============================================================================
echo "Phase 3: Deploy topology"
echo "----------------------------------------"

update_state "phase" "deploying"
update_state "deploy_start" "$(date -Iseconds)"

# Start memory watchdog BEFORE deployment
start_memory_watchdog

# Calculate max workers based on node count (fewer workers for larger deployments)
if [[ $NODE_COUNT -ge 384 ]]; then
    MAX_WORKERS=8
elif [[ $NODE_COUNT -ge 192 ]]; then
    MAX_WORKERS=12
elif [[ $NODE_COUNT -ge 96 ]]; then
    MAX_WORKERS=16
else
    MAX_WORKERS=20
fi

echo "Deploying with $MAX_WORKERS workers..."

# Deploy with timeout
if ! timeout 600 containerlab deploy -t "$TOPO_FILE" --reconfigure --max-workers "$MAX_WORKERS"; then
    echo "ERROR: Deployment failed or timed out"
    update_state "phase" "deploy_failed"
    exit 1
fi

update_state "deploy_end" "$(date -Iseconds)"

# Wait for containers to be running
echo "Waiting for containers..."
MAX_WAIT=120
WAIT_COUNT=0
while true; do
    RUNNING=$(docker ps --filter "name=clab-${CLAB_NAME}" --format "{{.Names}}" 2>/dev/null | wc -l)
    echo "  Containers running: $RUNNING / $EXPECTED_CONTAINERS"

    if [[ $RUNNING -ge $EXPECTED_CONTAINERS ]]; then
        echo "All containers running"
        break
    fi

    WAIT_COUNT=$((WAIT_COUNT + 1))
    if [[ $WAIT_COUNT -ge $MAX_WAIT ]]; then
        echo "WARNING: Timeout waiting for all containers (got $RUNNING / $EXPECTED_CONTAINERS)"
        update_state "container_warning" "timeout: $RUNNING / $EXPECTED_CONTAINERS"
        break
    fi

    sleep 2
done

update_state "phase" "deployed"
update_state "running_containers" "$RUNNING"

# Start log collector NOW - this is critical for surviving OOM
echo ""
echo "Starting streaming log collection..."
start_log_collector

# Initial log snapshot
collect_logs_once "initial"

echo ""

# ============================================================================
# PHASE 4: RUN TEST
# ============================================================================
echo "Phase 4: Run test (${TEST_DURATION_SECS}s)"
echo "----------------------------------------"

update_state "phase" "running"
update_state "test_start" "$(date -Iseconds)"

# Show progress
ELAPSED=0
INTERVAL=30
while [[ $ELAPSED -lt $TEST_DURATION_SECS ]]; do
    REMAINING=$((TEST_DURATION_SECS - ELAPSED))
    MEMORY_PCT=$(get_memory_pressure)
    CONTAINER_COUNT=$(docker ps --filter "name=clab-${CLAB_NAME}" --format "{{.Names}}" 2>/dev/null | wc -l)

    echo "[${ELAPSED}s/${TEST_DURATION_SECS}s] Containers: $CONTAINER_COUNT | Memory: ${MEMORY_PCT}%"

    # Check for container failures
    FAILED=$(docker ps -a --filter "name=clab-${CLAB_NAME}" --filter "status=exited" --format "{{.Names}}" | wc -l)
    if [[ $FAILED -gt 0 ]]; then
        echo "  WARNING: $FAILED containers have exited"
        update_state "failed_containers" "$FAILED"
    fi

    sleep $INTERVAL
    ELAPSED=$((ELAPSED + INTERVAL))
done

update_state "test_end" "$(date -Iseconds)"
update_state "phase" "test_complete"

echo ""
echo "Test complete!"
echo ""

# Final snapshot before cleanup
collect_logs_once "final"

# ============================================================================
# PHASE 5: SUMMARY
# ============================================================================
echo "Phase 5: Summary"
echo "----------------------------------------"

FINAL_CONTAINER_COUNT=$(docker ps --filter "name=clab-${CLAB_NAME}" --format "{{.Names}}" 2>/dev/null | wc -l)
FAILED_CONTAINERS=$(docker ps -a --filter "name=clab-${CLAB_NAME}" --filter "status=exited" --format "{{.Names}}" | wc -l)
LOG_SNAPSHOTS=$(find "$LOG_DIR" -maxdepth 1 -type d | wc -l)
TOTAL_LOGS=$(find "$LOG_DIR" -name "*.log" | wc -l)

echo "Containers: $FINAL_CONTAINER_COUNT running, $FAILED_CONTAINERS failed"
echo "Log snapshots: $LOG_SNAPSHOTS"
echo "Total log files: $TOTAL_LOGS"
echo ""
echo "Results directory: $RESULTS_DIR"

update_state "phase" "complete"
update_state "final_containers" "$FINAL_CONTAINER_COUNT"
update_state "failed_containers" "$FAILED_CONTAINERS"
update_state "log_snapshots" "$LOG_SNAPSHOTS"
update_state "total_logs" "$TOTAL_LOGS"

echo ""
echo "SUCCESS - cleanup will now run"
echo ""

# Cleanup runs via trap
