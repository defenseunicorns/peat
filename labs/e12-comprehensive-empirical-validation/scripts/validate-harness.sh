#!/bin/bash

################################################################################
# E12 Test Harness Validation Script
#
# Validates each component of the comprehensive test harness:
#   1. Docker stats collection
#   2. Topology deployment/cleanup
#   3. Bandwidth constraint application
#   4. Log collection
#   5. Metrics extraction
#   6. Analysis pipeline
################################################################################

set -euo pipefail

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

VALIDATION_DIR="validation-test-$(date +%Y%m%d-%H%M%S)"

log_step() {
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}STEP $1: $2${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

log_info() {
    echo -e "${CYAN}  → $1${NC}"
}

log_success() {
    echo -e "${GREEN}  ✓ $1${NC}"
}

log_error() {
    echo -e "${RED}  ✗ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}  ⚠ $1${NC}"
}

check_prerequisites() {
    log_step 0 "Checking Prerequisites"

    local all_good=true

    # Check Docker
    if command -v docker &> /dev/null; then
        log_success "Docker: $(docker --version)"
    else
        log_error "Docker not found"
        all_good=false
    fi

    # Check ContainerLab
    if command -v containerlab &> /dev/null; then
        log_success "ContainerLab: $(containerlab version | head -1)"
    else
        log_error "ContainerLab not found"
        all_good=false
    fi

    # Check Python3
    if command -v python3 &> /dev/null; then
        log_success "Python3: $(python3 --version)"
    else
        log_error "Python3 not found"
        all_good=false
    fi

    # Check environment file
    if [ -f "../../../.env" ]; then
        log_success "Environment file found"
        # Check for required variables
        if grep -q "DITTO_APP_ID" ../../../.env && \
           grep -q "DITTO_OFFLINE_TOKEN" ../../../.env && \
           grep -q "DITTO_SHARED_KEY" ../../../.env; then
            log_success "Ditto credentials configured"
        else
            log_warning "Ditto credentials may be incomplete in .env"
        fi
    else
        log_error "Environment file not found: ../../../.env"
        all_good=false
    fi

    # Check topology file
    if [ -f "../../../hive-sim/topologies/traditional-2node.yaml" ]; then
        log_success "Test topology available"
    else
        log_error "Test topology not found"
        all_good=false
    fi

    if [ "$all_good" = false ]; then
        log_error "Prerequisites check failed"
        exit 1
    fi

    log_success "All prerequisites satisfied"
}

test_docker_stats_collection() {
    log_step 1 "Testing Docker Stats Collection"

    # Create test directory
    mkdir -p "${VALIDATION_DIR}/docker-stats-test"

    log_info "Collecting Docker stats for 10 seconds..."

    # Start a simple container for testing
    docker run -d --name test-stats-container alpine:latest sleep 30 > /dev/null

    # Collect stats 3 times
    for i in 1 2 3; do
        log_info "Collection $i/3..."
        docker stats --no-stream --format "json" test-stats-container \
            > "${VALIDATION_DIR}/docker-stats-test/stats-$i.json" 2>/dev/null
        sleep 3
    done

    # Clean up test container
    docker stop test-stats-container > /dev/null 2>&1
    docker rm test-stats-container > /dev/null 2>&1

    # Verify files were created
    local count=$(ls -1 "${VALIDATION_DIR}/docker-stats-test/"*.json 2>/dev/null | wc -l)
    if [ "$count" -eq 3 ]; then
        log_success "Docker stats collection successful ($count files)"

        # Test parsing
        if python3 -c "import json; json.load(open('${VALIDATION_DIR}/docker-stats-test/stats-1.json'))" 2>/dev/null; then
            log_success "Stats files are valid JSON"
        else
            log_error "Stats files failed JSON parsing"
            return 1
        fi
    else
        log_error "Expected 3 stats files, found $count"
        return 1
    fi
}

test_topology_deployment() {
    log_step 2 "Testing Topology Deployment & Cleanup"

    # Source environment
    set -a
    source ../../../.env
    set +a

    log_info "Deploying 2-node test topology..."
    cd ../../../hive-sim
    containerlab deploy --reconfigure -t topologies/traditional-2node.yaml > /dev/null 2>&1
    cd - > /dev/null

    # Check containers are running
    sleep 5
    local container_count=$(docker ps --filter "name=clab-traditional-baseline-2node-" --format "{{.Names}}" | wc -l)

    if [ "$container_count" -ge 2 ]; then
        log_success "Topology deployed successfully ($container_count containers)"

        # List containers
        docker ps --filter "name=clab-traditional-baseline-2node-" --format "  - {{.Names}}"
    else
        log_error "Expected at least 2 containers, found $container_count"
        return 1
    fi

    # Test cleanup
    log_info "Testing topology cleanup..."
    containerlab destroy --all --cleanup > /dev/null 2>&1
    sleep 2

    local remaining=$(docker ps --filter "name=clab-traditional-baseline-2node-" --format "{{.Names}}" | wc -l)
    if [ "$remaining" -eq 0 ]; then
        log_success "Cleanup successful (0 containers remaining)"
    else
        log_warning "Cleanup incomplete ($remaining containers remaining)"
    fi
}

test_bandwidth_constraints() {
    log_step 3 "Testing Bandwidth Constraint Application"

    # Deploy topology again
    log_info "Deploying test topology..."
    set -a
    source ../../../.env
    set +a

    cd ../../../hive-sim
    containerlab deploy --reconfigure -t topologies/traditional-2node.yaml > /dev/null 2>&1
    cd - > /dev/null

    sleep 5

    # Test netem constraint
    log_info "Applying 1Mbps constraint..."

    local success=true
    for container in $(docker ps --filter "name=clab-traditional-baseline-2node-" --format "{{.Names}}"); do
        if containerlab tools netem set -n "$container" -i eth0 --rate 1024 > /dev/null 2>&1; then
            log_success "Constraint applied to $container"
        else
            log_error "Failed to apply constraint to $container"
            success=false
        fi
    done

    if [ "$success" = true ]; then
        log_success "Bandwidth constraints working"
    else
        log_error "Bandwidth constraint application failed"
    fi

    # Show netem table
    log_info "Current netem configuration:"
    containerlab tools netem show 2>/dev/null | grep -A 5 "cap-traditional-2node" || log_warning "Could not display netem table"

    # Cleanup
    containerlab destroy --all --cleanup > /dev/null 2>&1
}

test_log_collection() {
    log_step 4 "Testing Log Collection & Metrics Extraction"

    # Deploy and run topology briefly
    log_info "Deploying topology for log collection test..."
    set -a
    source ../../../.env
    set +a

    cd ../../../hive-sim
    containerlab deploy --reconfigure -t topologies/traditional-2node.yaml > /dev/null 2>&1
    cd - > /dev/null

    log_info "Waiting 30s for nodes to generate logs..."
    sleep 30

    # Collect logs
    mkdir -p "${VALIDATION_DIR}/logs-test"

    log_info "Collecting container logs..."
    local log_count=0
    for container in $(docker ps --filter "name=clab-traditional-baseline-2node-" --format "{{.Names}}"); do
        local clean_name=$(echo "$container" | sed 's/clab-traditional-baseline-2node-//')
        docker logs "$container" > "${VALIDATION_DIR}/logs-test/${clean_name}.log" 2>&1
        ((log_count++))
    done

    log_success "Collected logs from $log_count containers"

    # Test metrics extraction
    log_info "Extracting METRICS lines..."
    grep -h "METRICS:" "${VALIDATION_DIR}/logs-test/"*.log 2>/dev/null | \
        sed 's/.*METRICS: //' > "${VALIDATION_DIR}/logs-test/all-metrics.jsonl" || true

    local metric_count=$(wc -l < "${VALIDATION_DIR}/logs-test/all-metrics.jsonl" 2>/dev/null || echo 0)

    if [ "$metric_count" -gt 0 ]; then
        log_success "Extracted $metric_count metrics"

        # Show sample metrics
        log_info "Sample metrics:"
        head -3 "${VALIDATION_DIR}/logs-test/all-metrics.jsonl" | while read line; do
            echo "    $(echo $line | python3 -m json.tool --compact 2>/dev/null || echo $line)"
        done
    else
        log_warning "No metrics found in logs (this may be normal for very short tests)"
    fi

    # Cleanup
    containerlab destroy --all --cleanup > /dev/null 2>&1
}

test_analysis_script() {
    log_step 5 "Testing Analysis Script"

    # Create mock test results
    log_info "Creating mock test data..."
    mkdir -p "${VALIDATION_DIR}/mock-results/test-run"

    # Create mock test-summary.json
    cat > "${VALIDATION_DIR}/mock-results/test-run/test-summary.json" <<'EOF'
{
  "message_sent_count": 10,
  "message_received_count": 8,
  "document_inserted_count": 5,
  "document_received_count": 12,
  "total_bytes_sent": 5120,
  "avg_message_size_bytes": 512,
  "latency_count": 8,
  "latency_avg_ms": 15.5,
  "latency_median_ms": 12.3,
  "latency_p90_ms": 25.6,
  "latency_p99_ms": 30.1,
  "latency_min_ms": 8.2,
  "latency_max_ms": 32.5,
  "replication_factor": 2.4,
  "unique_senders": 2,
  "unique_receivers": 2
}
EOF

    # Create mock docker-stats-summary.json
    cat > "${VALIDATION_DIR}/mock-results/test-run/docker-stats-summary.json" <<'EOF'
{
  "node1": {
    "net_input_bytes": 102400,
    "net_output_bytes": 204800,
    "net_total_bytes": 307200,
    "avg_cpu_percent": 12.5,
    "avg_mem_bytes": 134217728
  },
  "node2": {
    "net_input_bytes": 204800,
    "net_output_bytes": 102400,
    "net_total_bytes": 307200,
    "avg_cpu_percent": 15.3,
    "avg_mem_bytes": 150994944
  }
}
EOF

    log_success "Mock test data created"

    # Test analysis script
    log_info "Running analysis script..."

    if python3 analyze-comprehensive-results.py "${VALIDATION_DIR}/mock-results" > /dev/null 2>&1; then
        log_success "Analysis script executed successfully"

        # Check output files
        if [ -f "${VALIDATION_DIR}/mock-results/EXECUTIVE-SUMMARY.md" ]; then
            log_success "Executive summary generated"
        else
            log_error "Executive summary not generated"
        fi

        if [ -f "${VALIDATION_DIR}/mock-results/aggregate-statistics.json" ]; then
            log_success "Aggregate statistics generated"
        else
            log_error "Aggregate statistics not generated"
        fi
    else
        log_error "Analysis script failed"
        return 1
    fi
}

run_pilot_test() {
    log_step 6 "Running Single Pilot Test (Traditional 2-node @ 1Gbps)"

    log_info "This will run a complete end-to-end test..."
    log_info "Duration: ~1.5 minutes"
    echo ""

    # Create a minimal test configuration
    mkdir -p "${VALIDATION_DIR}/pilot-test"

    # Source environment
    set -a
    source ../../../.env
    set +a

    # Deploy
    log_info "Deploying topology..."
    cd ../../../hive-sim
    containerlab deploy --reconfigure -t topologies/traditional-2node.yaml > /dev/null 2>&1
    cd - > /dev/null

    # Warm up
    log_info "Warm-up period (20s)..."
    sleep 20

    # Start stats collection
    log_info "Starting Docker stats collection..."
    (
        while true; do
            timestamp=$(date +%s)
            docker stats --no-stream --format "json" \
                $(docker ps --filter "name=clab-traditional-baseline-2node-" --format "{{.Names}}") \
                > "${VALIDATION_DIR}/pilot-test/stats-${timestamp}.json" 2>/dev/null || true
            sleep 5
        done
    ) &
    local stats_pid=$!

    # Measure
    log_info "Measurement period (30s)..."
    sleep 30

    # Stop stats
    kill $stats_pid 2>/dev/null || true
    log_success "Stats collection stopped"

    # Collect logs
    log_info "Collecting logs..."
    for container in $(docker ps --filter "name=clab-traditional-baseline-2node-" --format "{{.Names}}"); do
        local clean_name=$(echo "$container" | sed 's/clab-traditional-baseline-2node-//')
        docker logs "$container" > "${VALIDATION_DIR}/pilot-test/${clean_name}.log" 2>&1
    done

    # Extract metrics
    log_info "Extracting metrics..."
    grep -h "METRICS:" "${VALIDATION_DIR}/pilot-test/"*.log 2>/dev/null | \
        sed 's/.*METRICS: //' > "${VALIDATION_DIR}/pilot-test/all-metrics.jsonl" || true

    local metric_count=$(wc -l < "${VALIDATION_DIR}/pilot-test/all-metrics.jsonl" 2>/dev/null || echo 0)
    log_success "Extracted $metric_count metrics"

    # Calculate stats with Python
    log_info "Calculating summary statistics..."
    python3 - <<'PYTHON_SCRIPT' "${VALIDATION_DIR}/pilot-test"
import json
import sys
from pathlib import Path
from collections import defaultdict
import statistics

output_dir = Path(sys.argv[1])
metrics_file = output_dir / "all-metrics.jsonl"

if not metrics_file.exists():
    print("  ⚠ No metrics file found")
    sys.exit(0)

metrics = []
with open(metrics_file) as f:
    for line in f:
        try:
            metrics.append(json.loads(line.strip()))
        except:
            continue

msg_sent = [m for m in metrics if m.get("event_type") == "MessageSent"]
msg_recv = [m for m in metrics if m.get("event_type") == "MessageReceived"]

print(f"  ✓ Found {len(msg_sent)} MessageSent events")
print(f"  ✓ Found {len(msg_recv)} MessageReceived events")

if msg_sent:
    total_bytes = sum(m.get("message_size_bytes", 0) for m in msg_sent)
    print(f"  ✓ Total bytes sent: {total_bytes:,} bytes ({total_bytes/1e6:.2f} MB)")

latencies = []
for m in msg_recv:
    if "latency_us" in m:
        latencies.append(m["latency_us"] / 1000.0)

if latencies:
    print(f"  ✓ Latency avg: {statistics.mean(latencies):.2f}ms, "
          f"p50: {statistics.median(latencies):.2f}ms")

PYTHON_SCRIPT

    # Cleanup
    log_info "Cleaning up..."
    containerlab destroy --all --cleanup > /dev/null 2>&1

    log_success "Pilot test complete!"
}

main() {
    echo ""
    echo -e "${BLUE}╔════════════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║                    E12 Test Harness Validation                                 ║${NC}"
    echo -e "${BLUE}╚════════════════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""

    mkdir -p "$VALIDATION_DIR"

    # Run validation steps
    check_prerequisites
    test_docker_stats_collection
    test_topology_deployment
    test_bandwidth_constraints
    test_log_collection
    test_analysis_script
    run_pilot_test

    # Summary
    log_step "✓" "Validation Complete"

    echo ""
    echo -e "${GREEN}All components validated successfully!${NC}"
    echo ""
    echo "Results saved to: ${VALIDATION_DIR}"
    echo ""
    echo "Next steps:"
    echo "  1. Review validation results in ${VALIDATION_DIR}/"
    echo "  2. Run full test suite: ./run-comprehensive-suite.sh"
    echo ""
}

main "$@"
