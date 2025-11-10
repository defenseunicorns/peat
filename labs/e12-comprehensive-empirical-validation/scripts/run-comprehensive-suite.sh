#!/bin/bash

################################################################################
# E12 Comprehensive Empirical Validation - Test Harness
#
# Executes complete experimental matrix:
#   3 Architectures × Variable Scales × 4 Bandwidth Constraints
#
# Collects comprehensive metrics:
#   - Application-level metrics (JSONL from logs)
#   - Docker network statistics (bytes tx/rx)
#   - Per-node bandwidth usage
#   - Latency measurements
################################################################################

set -euo pipefail

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Timestamps
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="e12-comprehensive-results-${TIMESTAMP}"

# Test configuration
WARM_UP_SECONDS=30
MEASUREMENT_SECONDS=60
MEASUREMENT_SECONDS_CONSTRAINED=90  # Longer for constrained bandwidth
COOLDOWN_SECONDS=10

# Statistics collection interval (seconds)
STATS_INTERVAL=5

################################################################################
# Helper Functions
################################################################################

log_info() {
    echo -e "${CYAN}→ $1${NC}"
}

log_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

log_error() {
    echo -e "${RED}✗ $1${NC}"
}

log_section() {
    echo ""
    echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
    echo ""
}

################################################################################
# Docker Statistics Collection
################################################################################

start_docker_stats_collection() {
    local lab_name=$1
    local output_dir=$2
    local interval=$3

    log_info "Starting Docker stats collection (${interval}s interval)..."

    # Create stats directory
    mkdir -p "${output_dir}/docker-stats"

    # Start background stats collector
    (
        while true; do
            timestamp=$(date +%s)
            docker stats --no-stream --format "json" \
                $(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}") \
                > "${output_dir}/docker-stats/stats-${timestamp}.json" 2>/dev/null || true
            sleep "$interval"
        done
    ) &

    # Store PID for cleanup
    echo $! > "${output_dir}/stats-collector.pid"
}

stop_docker_stats_collection() {
    local output_dir=$1

    if [ -f "${output_dir}/stats-collector.pid" ]; then
        local pid=$(cat "${output_dir}/stats-collector.pid")
        kill "$pid" 2>/dev/null || true
        rm "${output_dir}/stats-collector.pid"
        log_info "Stopped Docker stats collection"
    fi
}

aggregate_docker_stats() {
    local output_dir=$1

    log_info "Aggregating Docker network statistics..."

    # Use Python to aggregate JSON stats files
    python3 - <<'PYTHON_SCRIPT' "$output_dir"
import json
import sys
from pathlib import Path
from collections import defaultdict

output_dir = Path(sys.argv[1])
stats_dir = output_dir / "docker-stats"

if not stats_dir.exists():
    print("No stats directory found")
    sys.exit(0)

# Aggregate stats across all collection points
node_stats = defaultdict(lambda: {
    "net_input_bytes": [],
    "net_output_bytes": [],
    "cpu_percent": [],
    "mem_usage_bytes": []
})

for stats_file in sorted(stats_dir.glob("stats-*.json")):
    try:
        with open(stats_file) as f:
            stats = json.load(f)

        # Handle both single object and array of objects
        if isinstance(stats, dict):
            stats = [stats]

        for container in stats:
            name = container.get("Name", "unknown")

            # Parse network I/O (format: "1.23MB / 4.56MB")
            net_io = container.get("NetIO", "0B / 0B")
            if " / " in net_io:
                input_str, output_str = net_io.split(" / ")

                def parse_bytes(s):
                    s = s.strip()
                    multipliers = {"B": 1, "kB": 1e3, "MB": 1e6, "GB": 1e9}
                    for suffix, mult in multipliers.items():
                        if suffix in s:
                            return float(s.replace(suffix, "")) * mult
                    return 0

                node_stats[name]["net_input_bytes"].append(parse_bytes(input_str))
                node_stats[name]["net_output_bytes"].append(parse_bytes(output_str))

            # Parse CPU percentage (format: "12.34%")
            cpu_str = container.get("CPUPerc", "0%").replace("%", "")
            try:
                node_stats[name]["cpu_percent"].append(float(cpu_str))
            except:
                pass

            # Parse memory usage (format: "123.4MiB / 456.7MiB")
            mem_usage = container.get("MemUsage", "0B / 0B")
            if " / " in mem_usage:
                usage_str = mem_usage.split(" / ")[0].strip()
                node_stats[name]["mem_usage_bytes"].append(parse_bytes(usage_str))

    except Exception as e:
        continue

# Calculate aggregates and write summary
summary = {}
for node, stats in node_stats.items():
    if stats["net_input_bytes"]:
        # Network: use max values (cumulative counters)
        max_input = max(stats["net_input_bytes"])
        max_output = max(stats["net_output_bytes"])

        # CPU/Memory: use average
        avg_cpu = sum(stats["cpu_percent"]) / len(stats["cpu_percent"]) if stats["cpu_percent"] else 0
        avg_mem = sum(stats["mem_usage_bytes"]) / len(stats["mem_usage_bytes"]) if stats["mem_usage_bytes"] else 0

        summary[node] = {
            "net_input_bytes": int(max_input),
            "net_output_bytes": int(max_output),
            "net_total_bytes": int(max_input + max_output),
            "avg_cpu_percent": round(avg_cpu, 2),
            "avg_mem_bytes": int(avg_mem)
        }

# Write summary
with open(output_dir / "docker-stats-summary.json", "w") as f:
    json.dump(summary, f, indent=2)

# Write human-readable report
with open(output_dir / "docker-stats-summary.txt", "w") as f:
    f.write("Docker Network Statistics Summary\n")
    f.write("=" * 80 + "\n\n")

    total_input = sum(s["net_input_bytes"] for s in summary.values())
    total_output = sum(s["net_output_bytes"] for s in summary.values())
    total_combined = total_input + total_output

    f.write(f"Total Network I/O:\n")
    f.write(f"  Input:  {total_input:,} bytes ({total_input/1e6:.2f} MB)\n")
    f.write(f"  Output: {total_output:,} bytes ({total_output/1e6:.2f} MB)\n")
    f.write(f"  Total:  {total_combined:,} bytes ({total_combined/1e6:.2f} MB)\n\n")

    f.write(f"Per-Node Breakdown:\n")
    f.write("-" * 80 + "\n")

    for node in sorted(summary.keys()):
        stats = summary[node]
        f.write(f"{node}:\n")
        f.write(f"  Network: {stats['net_total_bytes']:,} bytes " +
                f"(↓{stats['net_input_bytes']:,} ↑{stats['net_output_bytes']:,})\n")
        f.write(f"  CPU: {stats['avg_cpu_percent']:.1f}%  Memory: {stats['avg_mem_bytes']/1e6:.1f} MB\n")
        f.write("\n")

print(f"Aggregated stats for {len(summary)} nodes")
print(f"Total network I/O: {total_combined/1e6:.2f} MB")

PYTHON_SCRIPT

    log_success "Docker stats aggregated"
}

################################################################################
# Bandwidth Constraint Application
################################################################################

apply_bandwidth_constraint() {
    local lab_name=$1
    local bandwidth=$2

    # Convert bandwidth label to Kbps
    local rate_kbps
    case "$bandwidth" in
        "1gbps")
            rate_kbps=1048576
            ;;
        "100mbps")
            rate_kbps=102400
            ;;
        "1mbps")
            rate_kbps=1024
            ;;
        "256kbps")
            rate_kbps=256
            ;;
        "unconstrained")
            log_info "Skipping bandwidth constraint (unconstrained test)"
            return 0
            ;;
        *)
            log_error "Unknown bandwidth: $bandwidth"
            return 1
            ;;
    esac

    log_info "Applying ${bandwidth} constraint (${rate_kbps} Kbps) to all nodes..."

    for container in $(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}"); do
        containerlab tools netem set -n "$container" -i eth0 --rate "$rate_kbps" > /dev/null 2>&1 || true
    done

    log_success "Bandwidth constraint applied"
}

################################################################################
# Topology Deployment
################################################################################

deploy_topology() {
    local topology_file=$1
    local env_file=$2

    log_info "Deploying topology: $(basename $topology_file)"

    # Source environment variables
    if [ -f "$env_file" ]; then
        set -a
        source "$env_file"
        set +a
    else
        log_error "Environment file not found: $env_file"
        return 1
    fi

    # Deploy using containerlab
    cd "$(dirname "$topology_file")"
    containerlab deploy -t "$(basename "$topology_file")" > /dev/null 2>&1
    cd - > /dev/null

    log_success "Topology deployed"
}

destroy_topology() {
    log_info "Destroying topology..."
    timeout 60 containerlab destroy --all --cleanup > /dev/null 2>&1 || true
    log_success "Topology destroyed"
}

################################################################################
# Metrics Collection
################################################################################

collect_logs() {
    local lab_name=$1
    local output_dir=$2

    log_info "Collecting container logs..."

    mkdir -p "$output_dir"

    for container in $(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}"); do
        # Strip clab prefix for cleaner filenames
        local clean_name=$(echo "$container" | sed "s/clab-${lab_name}-//")
        docker logs "$container" > "${output_dir}/${clean_name}.log" 2>&1
    done

    log_success "Logs collected"
}

extract_metrics() {
    local output_dir=$1

    log_info "Extracting metrics from logs..."

    # Extract all METRICS: lines to JSONL
    grep -h "METRICS:" "${output_dir}"/*.log 2>/dev/null | \
        sed 's/.*METRICS: //' > "${output_dir}/all-metrics.jsonl" || true

    # Count metrics
    local metric_count=$(wc -l < "${output_dir}/all-metrics.jsonl" 2>/dev/null || echo 0)

    log_success "Extracted ${metric_count} metrics"
}

calculate_test_metrics() {
    local output_dir=$1

    log_info "Calculating summary statistics..."

    python3 - <<'PYTHON_SCRIPT' "$output_dir"
import json
import sys
from pathlib import Path
from collections import defaultdict
import statistics

output_dir = Path(sys.argv[1])
metrics_file = output_dir / "all-metrics.jsonl"

if not metrics_file.exists():
    print("No metrics file found")
    sys.exit(0)

# Parse all metrics
metrics = []
with open(metrics_file) as f:
    for line in f:
        try:
            metrics.append(json.loads(line.strip()))
        except:
            continue

# Categorize metrics
msg_sent = [m for m in metrics if m.get("event_type") == "MessageSent"]
msg_recv = [m for m in metrics if m.get("event_type") == "MessageReceived"]
doc_inserted = [m for m in metrics if m.get("event_type") == "DocumentInserted"]
doc_received = [m for m in metrics if m.get("event_type") == "DocumentReceived"]

# Calculate statistics
summary = {
    "message_sent_count": len(msg_sent),
    "message_received_count": len(msg_recv),
    "document_inserted_count": len(doc_inserted),
    "document_received_count": len(doc_received),
}

# Bandwidth (from MessageSent)
if msg_sent:
    total_bytes = sum(m.get("message_size_bytes", 0) for m in msg_sent)
    summary["total_bytes_sent"] = total_bytes
    summary["avg_message_size_bytes"] = total_bytes / len(msg_sent)

# Latency (from DocumentReceived and MessageReceived)
latencies = []
for m in doc_received + msg_recv:
    if "latency_us" in m and m["latency_us"] < 10000000:  # Filter outliers (>10s)
        latencies.append(m["latency_us"] / 1000.0)  # Convert to ms

if latencies:
    latencies.sort()
    summary["latency_count"] = len(latencies)
    summary["latency_avg_ms"] = statistics.mean(latencies)
    summary["latency_median_ms"] = statistics.median(latencies)
    summary["latency_p90_ms"] = latencies[int(len(latencies) * 0.9)] if len(latencies) > 10 else latencies[-1]
    summary["latency_p99_ms"] = latencies[int(len(latencies) * 0.99)] if len(latencies) > 100 else latencies[-1]
    summary["latency_min_ms"] = min(latencies)
    summary["latency_max_ms"] = max(latencies)

# Per-node breakdown
nodes_sent = defaultdict(int)
nodes_received = defaultdict(int)
for m in msg_sent:
    nodes_sent[m.get("node_id", "unknown")] += 1
for m in doc_received:
    nodes_received[m.get("node_id", "unknown")] += 1

summary["unique_senders"] = len(nodes_sent)
summary["unique_receivers"] = len(nodes_received)

# Document replication factor
if doc_inserted:
    summary["replication_factor"] = len(doc_received) / len(doc_inserted)

# Write summary
with open(output_dir / "test-summary.json", "w") as f:
    json.dump(summary, f, indent=2)

# Write human-readable summary
with open(output_dir / "test-summary.txt", "w") as f:
    f.write("Test Metrics Summary\n")
    f.write("=" * 80 + "\n\n")

    f.write("Message Counts:\n")
    f.write(f"  Messages sent: {summary.get('message_sent_count', 0)}\n")
    f.write(f"  Messages received: {summary.get('message_received_count', 0)}\n")
    f.write(f"  Documents inserted: {summary.get('document_inserted_count', 0)}\n")
    f.write(f"  Documents received: {summary.get('document_received_count', 0)}\n\n")

    if "total_bytes_sent" in summary:
        f.write("Bandwidth:\n")
        f.write(f"  Total bytes sent: {summary['total_bytes_sent']:,} bytes ")
        f.write(f"({summary['total_bytes_sent']/1e6:.2f} MB)\n")
        f.write(f"  Average message size: {summary['avg_message_size_bytes']:.1f} bytes\n\n")

    if "latency_avg_ms" in summary:
        f.write("Latency:\n")
        f.write(f"  Measurements: {summary['latency_count']}\n")
        f.write(f"  Average: {summary['latency_avg_ms']:.2f} ms\n")
        f.write(f"  Median (p50): {summary['latency_median_ms']:.2f} ms\n")
        f.write(f"  p90: {summary['latency_p90_ms']:.2f} ms\n")
        f.write(f"  p99: {summary['latency_p99_ms']:.2f} ms\n")
        f.write(f"  Range: {summary['latency_min_ms']:.2f} - {summary['latency_max_ms']:.2f} ms\n\n")

    if "replication_factor" in summary:
        f.write("Replication:\n")
        f.write(f"  Replication factor: {summary['replication_factor']:.2f}x\n")
        f.write(f"  (Each document received by {summary['replication_factor']:.1f} nodes on average)\n\n")

    f.write("Node Participation:\n")
    f.write(f"  Unique senders: {summary.get('unique_senders', 0)}\n")
    f.write(f"  Unique receivers: {summary.get('unique_receivers', 0)}\n")

print(f"Summary statistics calculated: {len(summary)} metrics")

PYTHON_SCRIPT

    log_success "Summary statistics calculated"
}

################################################################################
# Test Execution
################################################################################

run_single_test() {
    local architecture=$1
    local scale=$2
    local bandwidth=$3
    local topology_file=$4

    local test_name="${architecture}-${scale}-${bandwidth}"
    local test_output="${RESULTS_DIR}/${test_name}"

    log_section "TEST: ${test_name}"

    # Create output directory
    mkdir -p "$test_output"

    # Record test configuration
    cat > "${test_output}/test-config.txt" <<EOF
Architecture: $architecture
Scale: $scale
Bandwidth: $bandwidth
Topology: $(basename $topology_file)
Start Time: $(date --iso-8601=seconds)
EOF

    # Deploy topology
    deploy_topology "$topology_file" "../../../.env" || {
        log_error "Failed to deploy topology"
        return 1
    }

    # Extract lab name from topology file
    local lab_name=$(grep "^name:" "$topology_file" | awk '{print $2}')

    # Warm-up period
    log_info "Warming up (${WARM_UP_SECONDS}s)..."
    sleep "$WARM_UP_SECONDS"

    # Apply bandwidth constraint
    if [ "$bandwidth" != "unconstrained" ]; then
        apply_bandwidth_constraint "$lab_name" "$bandwidth"
        log_info "Constraint stabilization (${COOLDOWN_SECONDS}s)..."
        sleep "$COOLDOWN_SECONDS"
    fi

    # Start Docker stats collection
    start_docker_stats_collection "$lab_name" "$test_output" "$STATS_INTERVAL"

    # Measurement period
    local measurement_duration="$MEASUREMENT_SECONDS"
    if [ "$bandwidth" = "1mbps" ] || [ "$bandwidth" = "256kbps" ]; then
        measurement_duration="$MEASUREMENT_SECONDS_CONSTRAINED"
    fi

    log_info "Measuring (${measurement_duration}s)..."
    sleep "$measurement_duration"

    # Stop Docker stats collection
    stop_docker_stats_collection "$test_output"

    # Collect logs and metrics
    collect_logs "$lab_name" "$test_output"
    extract_metrics "$test_output"
    calculate_test_metrics "$test_output"
    aggregate_docker_stats "$test_output"

    # Record completion
    echo "End Time: $(date --iso-8601=seconds)" >> "${test_output}/test-config.txt"

    # Destroy topology
    destroy_topology

    log_success "Test complete: ${test_name}"
    echo ""
}

################################################################################
# Test Configurations
################################################################################

# Architecture → Scale → Bandwidth → Topology File
declare -A TEST_CONFIGS

# Traditional IoT configurations
TEST_CONFIGS["traditional-2node-1gbps"]="../../../cap-sim/topologies/traditional-2node.yaml"
TEST_CONFIGS["traditional-2node-100mbps"]="../../../cap-sim/topologies/traditional-2node.yaml"
TEST_CONFIGS["traditional-2node-1mbps"]="../../../cap-sim/topologies/traditional-2node.yaml"
TEST_CONFIGS["traditional-2node-256kbps"]="../../../cap-sim/topologies/traditional-2node.yaml"

TEST_CONFIGS["traditional-12node-1gbps"]="../../../cap-sim/topologies/traditional-squad-client-server.yaml"
TEST_CONFIGS["traditional-12node-100mbps"]="../../../cap-sim/topologies/traditional-squad-client-server.yaml"
TEST_CONFIGS["traditional-12node-1mbps"]="../../../cap-sim/topologies/traditional-squad-client-server.yaml"
TEST_CONFIGS["traditional-12node-256kbps"]="../../../cap-sim/topologies/traditional-squad-client-server.yaml"

TEST_CONFIGS["traditional-24node-1gbps"]="../../../cap-sim/topologies/platoon-24node-client-server-mode4.yaml"
TEST_CONFIGS["traditional-24node-100mbps"]="../../../cap-sim/topologies/platoon-24node-client-server-mode4.yaml"
TEST_CONFIGS["traditional-24node-1mbps"]="../../../cap-sim/topologies/platoon-24node-client-server-mode4.yaml"
TEST_CONFIGS["traditional-24node-256kbps"]="../../../cap-sim/topologies/platoon-24node-client-server-mode4.yaml"

# CAP Full Mesh configurations (no aggregation)
TEST_CONFIGS["cap-full-12node-1gbps"]="../../../cap-sim/topologies/squad-12node-dynamic-mesh.yaml"
TEST_CONFIGS["cap-full-12node-100mbps"]="../../../cap-sim/topologies/squad-12node-dynamic-mesh.yaml"
TEST_CONFIGS["cap-full-12node-1mbps"]="../../../cap-sim/topologies/squad-12node-dynamic-mesh.yaml"
TEST_CONFIGS["cap-full-12node-256kbps"]="../../../cap-sim/topologies/squad-12node-dynamic-mesh.yaml"

TEST_CONFIGS["cap-full-24node-1gbps"]="../../../cap-sim/topologies/platoon-24node-mesh-mode4.yaml"
TEST_CONFIGS["cap-full-24node-100mbps"]="../../../cap-sim/topologies/platoon-24node-mesh-mode4.yaml"
TEST_CONFIGS["cap-full-24node-1mbps"]="../../../cap-sim/topologies/platoon-24node-mesh-mode4.yaml"
TEST_CONFIGS["cap-full-24node-256kbps"]="../../../cap-sim/topologies/platoon-24node-mesh-mode4.yaml"

# CAP Hierarchical configurations (with aggregation)
TEST_CONFIGS["cap-hierarchical-24node-1gbps"]="../../../cap-sim/topologies/platoon-24node-mesh-mode4.yaml"
TEST_CONFIGS["cap-hierarchical-24node-100mbps"]="../../../cap-sim/topologies/platoon-24node-mesh-mode4.yaml"
TEST_CONFIGS["cap-hierarchical-24node-1mbps"]="../../../cap-sim/topologies/platoon-24node-mesh-mode4.yaml"
TEST_CONFIGS["cap-hierarchical-24node-256kbps"]="../../../cap-sim/topologies/platoon-24node-mesh-mode4.yaml"

################################################################################
# Main Execution
################################################################################

main() {
    log_section "E12 Comprehensive Empirical Validation"

    echo "Test Matrix:"
    echo "  • Architectures: Traditional IoT, CAP Full Mesh, CAP Hierarchical"
    echo "  • Scales: 2, 12, 24 nodes"
    echo "  • Bandwidths: 1Gbps, 100Mbps, 1Mbps, 256Kbps"
    echo ""
    echo "Total Tests: ${#TEST_CONFIGS[@]}"
    echo "Results Directory: ${RESULTS_DIR}"
    echo ""
    echo "Metrics Collected:"
    echo "  • Application-level metrics (JSONL)"
    echo "  • Docker network statistics (bytes tx/rx)"
    echo "  • Per-node bandwidth usage"
    echo "  • Latency measurements (p50, p90, p99)"
    echo ""

    # Create results directory
    mkdir -p "$RESULTS_DIR"

    # Execute all tests
    local test_number=1
    local total_tests=${#TEST_CONFIGS[@]}

    for test_config in "${!TEST_CONFIGS[@]}"; do
        local topology_file="${TEST_CONFIGS[$test_config]}"

        # Parse test configuration
        IFS='-' read -r architecture scale bandwidth <<< "$test_config"

        echo -e "${YELLOW}[${test_number}/${total_tests}]${NC}"

        # Run test
        run_single_test "$architecture" "$scale" "$bandwidth" "$topology_file" || {
            log_error "Test failed: ${test_config}"
        }

        ((test_number++))
    done

    log_section "Test Suite Complete"

    log_success "All tests completed"
    log_info "Results saved to: ${RESULTS_DIR}"

    echo ""
    echo "Next steps:"
    echo "  1. Analyze results: python3 scripts/analyze-comprehensive-results.py ${RESULTS_DIR}"
    echo "  2. Generate report: cd ${RESULTS_DIR} && cat COMPREHENSIVE-REPORT.md"
}

# Execute main function
main "$@"
