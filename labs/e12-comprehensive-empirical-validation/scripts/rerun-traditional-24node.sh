#!/bin/bash

# Rerun just the traditional-24node tests with the new baseline topology
# This script manually runs the 4 tests without sourcing the full suite

set -e

# Change to the scripts directory
cd "$(dirname "$0")"

# Load environment variables
if [ -f ../../../.env ]; then
    set -a
    source ../../../.env
    set +a
fi

# Use existing results directory
RESULTS_DIR="e12-comprehensive-results-20251110-115542"

if [ ! -d "$RESULTS_DIR" ]; then
    echo "Error: Results directory $RESULTS_DIR not found"
    exit 1
fi

# Test configurations
declare -A TEST_CONFIGS
TEST_CONFIGS["traditional-24node-1gbps"]="../../../cap-sim/topologies/traditional-platoon-24node.yaml"
TEST_CONFIGS["traditional-24node-100mbps"]="../../../cap-sim/topologies/traditional-platoon-24node.yaml"
TEST_CONFIGS["traditional-24node-1mbps"]="../../../cap-sim/topologies/traditional-platoon-24node.yaml"
TEST_CONFIGS["traditional-24node-256kbps"]="../../../cap-sim/topologies/traditional-platoon-24node.yaml"

# Bandwidth constraints
declare -A BANDWIDTH_CONSTRAINTS
BANDWIDTH_CONSTRAINTS["1gbps"]="1gbit"
BANDWIDTH_CONSTRAINTS["100mbps"]="100mbit"
BANDWIDTH_CONSTRAINTS["1mbps"]="1mbit"
BANDWIDTH_CONSTRAINTS["256kbps"]="256kbit"

# Test duration
TEST_DURATION=60

# Helper functions
log_info() {
    echo "→ $1"
}

log_success() {
    echo "✓ $1"
}

log_error() {
    echo "✗ $1"
}

deploy_topology() {
    local topology_file=$1
    log_info "Deploying topology: $topology_file"

    if ! containerlab deploy -t "$topology_file" --reconfigure > /dev/null 2>&1; then
        log_error "Failed to deploy topology"
        return 1
    fi

    sleep 5
    log_success "Topology deployed"
}

apply_bandwidth_constraints() {
    local bandwidth=$1
    local rate=${BANDWIDTH_CONSTRAINTS[$bandwidth]}

    if [ -z "$rate" ]; then
        log_error "Unknown bandwidth: $bandwidth"
        return 1
    fi

    log_info "Applying bandwidth constraint: $rate"

    # Get all containerlab containers
    local containers=$(docker ps --filter "name=clab-" --format "{{.Names}}")

    for container in $containers; do
        containerlab tools netem set -n "$container" --rate "$rate" > /dev/null 2>&1 || true
    done

    sleep 2
    log_success "Bandwidth constraints applied: $rate"
}

start_stats_collection() {
    local test_dir=$1
    local stats_dir="$test_dir/docker-stats"
    mkdir -p "$stats_dir"

    log_info "Starting Docker stats collection..."

    # Start background stats collection
    (
        while true; do
            timestamp=$(date +%s)
            containers=$(docker ps --filter "name=clab-" --format "{{.Names}}")
            if [ -n "$containers" ]; then
                docker stats --no-stream --format "json" $containers > "$stats_dir/stats-$timestamp.json" 2>/dev/null || true
            fi
            sleep 5
        done
    ) &

    echo $! > "$test_dir/stats_pid.txt"
    log_success "Stats collection started (PID: $(cat $test_dir/stats_pid.txt))"
}

stop_stats_collection() {
    local test_dir=$1

    if [ -f "$test_dir/stats_pid.txt" ]; then
        local pid=$(cat "$test_dir/stats_pid.txt")
        log_info "Stopping stats collection (PID: $pid)..."
        kill $pid 2>/dev/null || true
        rm "$test_dir/stats_pid.txt"
        log_success "Stats collection stopped"
    fi
}

collect_logs() {
    local test_dir=$1
    local logs_dir="$test_dir/logs"
    mkdir -p "$logs_dir"

    log_info "Collecting application logs..."

    local containers=$(docker ps --filter "name=clab-" --format "{{.Names}}")
    local count=0

    for container in $containers; do
        timeout 30 docker logs "$container" > "$logs_dir/${container}.log" 2>&1 || true
        count=$((count + 1))
    done

    log_success "Collected logs from $count containers"
}

destroy_topology() {
    log_info "Destroying topology..."

    # Force remove all containerlab containers directly
    docker ps -a --filter "name=clab-" --format "{{.Names}}" | xargs -r docker rm -f > /dev/null 2>&1 || true

    sleep 1
    log_success "Topology destroyed"
}

run_single_test() {
    local test_name=$1
    local topology_file=$2

    echo ""
    echo "========================================"
    echo "Running: $test_name"
    echo "========================================"

    # Extract bandwidth from test name
    local bandwidth="${test_name##*-}"

    # Create test directory
    local test_dir="$RESULTS_DIR/$test_name"

    # Remove old test results if they exist
    if [ -d "$test_dir" ]; then
        log_info "Removing old test results..."
        rm -rf "$test_dir"
    fi

    mkdir -p "$test_dir"

    # Record test configuration
    cat > "$test_dir/test-config.json" <<EOF
{
  "test_name": "$test_name",
  "topology_file": "$topology_file",
  "bandwidth": "$bandwidth",
  "duration": $TEST_DURATION,
  "start_time": "$(date -Iseconds)"
}
EOF

    # Deploy topology
    if ! deploy_topology "$topology_file"; then
        log_error "Failed to deploy topology for $test_name"
        return 1
    fi

    # Apply bandwidth constraints
    if ! apply_bandwidth_constraints "$bandwidth"; then
        log_error "Failed to apply bandwidth constraints for $test_name"
        destroy_topology
        return 1
    fi

    # Start stats collection
    start_stats_collection "$test_dir"

    # Wait for test duration
    log_info "Running test for ${TEST_DURATION}s..."
    sleep $TEST_DURATION

    # Stop stats collection
    stop_stats_collection "$test_dir"

    # Collect logs
    collect_logs "$test_dir"

    # Destroy topology
    destroy_topology

    # Update test configuration with end time
    local temp_file=$(mktemp)
    jq ". + {end_time: \"$(date -Iseconds)\"}" "$test_dir/test-config.json" > "$temp_file"
    mv "$temp_file" "$test_dir/test-config.json"

    log_success "Test completed: $test_name"
    echo ""
}

# Main execution
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Rerunning Traditional 24-Node Tests                      ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results directory: $RESULTS_DIR"
echo "Tests to run: ${#TEST_CONFIGS[@]}"
echo ""

# Run each test
for test_name in "${!TEST_CONFIGS[@]}"; do
    topology_file="${TEST_CONFIGS[$test_name]}"
    run_single_test "$test_name" "$topology_file"
done

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  All Traditional 24-Node Tests Completed                  ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Next step: Run analysis to generate updated comparative report"
echo "  make e12-analyze DIR=$RESULTS_DIR"
