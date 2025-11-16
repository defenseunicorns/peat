#!/bin/bash
# Test all topology modes with different bandwidth constraints
#
# Bandwidth levels:
# - 100Mbps (unconstrained baseline)
# - 10Mbps (typical WiFi/LTE)
# - 1Mbps (constrained wireless)
# - 256Kbps (tactical radio)

set -e

cd "$(dirname "$0")"

echo "======================================"
echo "E8 Bandwidth Constraint Testing"
echo "======================================"
echo ""

# Load environment variables
set -a
source ../.env 2>/dev/null || true
set +a

# Create results directory
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_BASE_DIR="test-results-bandwidth-$TIMESTAMP"
mkdir -p "$RESULTS_BASE_DIR"

echo "Results will be saved to: $RESULTS_BASE_DIR"
echo ""

# Bandwidth configurations (in Kbps for netem)
declare -A BANDWIDTHS=(
    ["100mbps"]=102400    # 100 Mbps = 102400 Kbps
    ["10mbps"]=10240      # 10 Mbps = 10240 Kbps
    ["1mbps"]=1024        # 1 Mbps = 1024 Kbps
    ["256kbps"]=256       # 256 Kbps
)

# Test modes
MODES=(
    "mode1-client-server:topologies/squad-12node-client-server.yaml"
    "mode2-hub-spoke:topologies/squad-12node-hub-spoke.yaml"
    "mode3-dynamic-mesh:topologies/squad-12node-dynamic-mesh.yaml"
)

# Function to apply bandwidth constraints to all nodes
apply_bandwidth_constraint() {
    local rate_kbps=$1
    local lab_name=$2

    echo "  Applying ${rate_kbps} Kbps constraint to all nodes..."

    # Get all container names for this lab
    local containers=$(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}")

    for container in $containers; do
        # Apply constraint to eth0 interface
        containerlab tools netem set -n "$container" -i eth0 --rate "$rate_kbps" 2>/dev/null || true
    done
}

# Function to clear bandwidth constraints
clear_bandwidth_constraints() {
    local lab_name=$1

    echo "  Clearing bandwidth constraints..."

    local containers=$(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}")

    for container in $containers; do
        # Use timeout to prevent hanging on netem delete
        timeout 5 containerlab tools netem delete -n "$container" -i eth0 2>/dev/null || true
    done
}

# Test each bandwidth configuration
for bw_name in "100mbps" "10mbps" "1mbps" "256kbps"; do
    bw_rate=${BANDWIDTHS[$bw_name]}

    echo "======================================"
    echo "Testing with $bw_name bandwidth"
    echo "======================================"
    echo ""

    # Create results subdirectory for this bandwidth
    BW_RESULTS_DIR="$RESULTS_BASE_DIR/$bw_name"
    mkdir -p "$BW_RESULTS_DIR"

    # Test each mode
    for mode_config in "${MODES[@]}"; do
        mode_name="${mode_config%%:*}"
        topology_file="${mode_config##*:}"

        echo "--------------------------------------"
        echo "Testing: $mode_name at $bw_name"
        echo "--------------------------------------"

        start_time=$(date +%s)

        # Deploy topology
        echo "[$(date +%T)] Deploying topology..."
        # Clean up any existing deployment of this specific topology
        containerlab destroy -t "$topology_file" --cleanup > /dev/null 2>&1 || true
        sleep 2

        deploy_start=$(date +%s)
        containerlab deploy -t "$topology_file" > "$BW_RESULTS_DIR/${mode_name}_deploy.log" 2>&1
        deploy_time=$(($(date +%s) - deploy_start))
        echo "[$(date +%T)] Deployment complete in ${deploy_time}s"

        # Extract lab name from topology file
        lab_name=$(grep "name:" "$topology_file" | head -1 | awk '{print $2}')

        # Wait for containers to start
        sleep 3

        # Apply bandwidth constraint
        echo "[$(date +%T)] Applying $bw_name bandwidth constraint..."
        apply_bandwidth_constraint "$bw_rate" "$lab_name"

        # Measure baseline bandwidth with iperf3
        echo "[$(date +%T)] Measuring actual bandwidth with iperf3..."
        bandwidth_mbps="0"
        containers=($(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}" | head -2))
        if [ ${#containers[@]} -ge 2 ]; then
            first_container="${containers[0]}"
            second_container="${containers[1]}"

            docker exec "$first_container" iperf3 -s -D > /dev/null 2>&1 || true
            sleep 1

            bandwidth_result=$(docker exec "$second_container" iperf3 -c "$first_container" -t 3 -J 2>/dev/null || echo "{}")
            bandwidth_mbps=$(echo "$bandwidth_result" | jq -r '.end.sum_received.bits_per_second // 0' 2>/dev/null | awk '{printf "%.2f", $1/1000000}')

            if [ "$bandwidth_mbps" == "0" ] || [ -z "$bandwidth_mbps" ]; then
                bandwidth_mbps="N/A"
            fi
        fi
        echo "  Measured bandwidth: ${bandwidth_mbps} Mbps"

        # Wait for test completion (40s as in original script)
        echo "[$(date +%T)] Waiting for node initialization and test completion (40s)..."
        sleep 40

        # Collect metrics
        echo "[$(date +%T)] Collecting metrics..."
        node_count=$(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}" | wc -l)
        echo "  Nodes running: $node_count"

        # Check sync status
        echo "[$(date +%T)] Checking sync status..."
        docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}" | while read container; do
            status="UNKNOWN"
            if docker logs "$container" 2>&1 | grep -q "POC SUCCESS"; then
                status="SYNCED"
            elif docker logs "$container" 2>&1 | grep -q "POC FAILED"; then
                status="FAILED"
            fi
            echo "  $container: $status"
        done > "$BW_RESULTS_DIR/${mode_name}_sync_status.txt"

        # Capture all container logs
        echo "[$(date +%T)] Capturing all container logs..."
        for container in $(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}"); do
            node_name=$(echo "$container" | sed 's/.*-\([^-]*-[0-9]*\)$/\1/')
            docker logs "$container" > "$BW_RESULTS_DIR/${mode_name}_${node_name}.log" 2>&1
        done

        # Analyze performance metrics
        echo "[$(date +%T)] Analyzing performance metrics..."
        log_files=""
        for log_file in "$BW_RESULTS_DIR/${mode_name}"_*.log; do
            [ -f "$log_file" ] && log_files="$log_files $log_file"
        done

        metrics_json="$BW_RESULTS_DIR/${mode_name}_metrics.json"
        if python3 analyze_metrics.py $log_files > "$metrics_json" 2>/dev/null; then
            echo "  ✓ Metrics analysis complete"

            convergence_ms=$(jq -r '.convergence.convergence_time_ms // 0' "$metrics_json" 2>/dev/null | awk '{printf "%.2f", $1}')
            latency_mean=$(jq -r '.latency.mean // 0' "$metrics_json" 2>/dev/null | awk '{printf "%.2f", $1}')
            latency_p90=$(jq -r '.latency.p90 // 0' "$metrics_json" 2>/dev/null | awk '{printf "%.2f", $1}')
            latency_p99=$(jq -r '.latency.p99 // 0' "$metrics_json" 2>/dev/null | awk '{printf "%.2f", $1}')
            round_trip_ms=$(jq -r '.acknowledgments.round_trip_latency_ms // 0' "$metrics_json" 2>/dev/null | awk '{printf "%.2f", $1}')
            ack_count=$(jq -r '.acknowledgments.ack_count // 0' "$metrics_json" 2>/dev/null)

            echo "  Convergence time: ${convergence_ms}ms"
            echo "  Latency: mean=${latency_mean}ms, p90=${latency_p90}ms, p99=${latency_p99}ms"
            echo "  Round-trip latency: ${round_trip_ms}ms (${ack_count} acks)"
        else
            convergence_ms="N/A"
            latency_mean="N/A"
            latency_p90="N/A"
            latency_p99="N/A"
            round_trip_ms="N/A"
            ack_count="0"
        fi

        # Save summary
        total_time=$(($(date +%s) - start_time))
        echo "[$(date +%T)] Test complete in ${total_time}s"

        cat > "$BW_RESULTS_DIR/${mode_name}_summary.txt" <<EOF
Mode: $mode_name
Bandwidth Constraint: $bw_name
Topology: $topology_file
Deploy Time: ${deploy_time}s
Total Test Time: ${total_time}s
Nodes Deployed: $node_count
Configured Bandwidth: $bw_name
Measured Bandwidth: ${bandwidth_mbps} Mbps
Convergence Time: ${convergence_ms}ms
Latency Mean: ${latency_mean}ms
Latency P90: ${latency_p90}ms
Latency P99: ${latency_p99}ms
Round-trip Latency: ${round_trip_ms}ms
Acknowledgments Received: ${ack_count}
Timestamp: $(date)
EOF

        # Clean up
        echo "[$(date +%T)] Cleaning up..."
        # Note: No need to clear netem constraints - destroying container cleans network namespace
        containerlab destroy -t "$topology_file" --cleanup > /dev/null 2>&1 || {
            echo "[$(date +%T)] Destroy failed, forcing cleanup..."
            docker rm -f $(docker ps -a -q --filter "name=clab-${lab_name}-" 2>/dev/null) 2>/dev/null || true
        }
        sleep 2

        echo ""
    done

    echo ""
done

# Generate comprehensive summary
echo "======================================"
echo "Generating comprehensive summary..."
echo "======================================"

SUMMARY_FILE="$RESULTS_BASE_DIR/COMPREHENSIVE_SUMMARY.md"

cat > "$SUMMARY_FILE" <<EOF
# E8 Bandwidth Constraint Testing - Comprehensive Results

**Test Date:** $(date)

## Test Overview

Tested three topology modes with bandwidth constraints:
- 100Mbps (unconstrained baseline)
- 10Mbps (typical WiFi/LTE)
- 1Mbps (constrained wireless)
- 256Kbps (tactical radio)

### Topology Modes:
1. **Mode 1: Client-Server** - All nodes connect to soldier-1 (central server)
2. **Mode 2: Hub-Spoke** - Hierarchical topology with team leaders
3. **Mode 3: Dynamic Mesh** - Full peer-to-peer mesh topology

## Results by Bandwidth

EOF

for bw_name in "100mbps" "10mbps" "1mbps" "256kbps"; do
    cat >> "$SUMMARY_FILE" <<EOF

### $bw_name Results

| Mode | Convergence (ms) | Mean Latency (ms) | P90 Latency (ms) | P99 Latency (ms) | Measured BW (Mbps) |
|------|------------------|-------------------|------------------|------------------|--------------------|
EOF

    for mode_config in "${MODES[@]}"; do
        mode_name="${mode_config%%:*}"
        summary_file="$RESULTS_BASE_DIR/$bw_name/${mode_name}_summary.txt"

        if [ -f "$summary_file" ]; then
            convergence=$(grep "Convergence Time:" "$summary_file" | cut -d: -f2 | tr -d ' ')
            mean_lat=$(grep "Latency Mean:" "$summary_file" | cut -d: -f2 | tr -d ' ')
            p90_lat=$(grep "Latency P90:" "$summary_file" | cut -d: -f2 | tr -d ' ')
            p99_lat=$(grep "Latency P99:" "$summary_file" | cut -d: -f2 | tr -d ' ')
            measured_bw=$(grep "Measured Bandwidth:" "$summary_file" | cut -d: -f2 | tr -d ' ')

            echo "| $mode_name | $convergence | $mean_lat | $p90_lat | $p99_lat | $measured_bw |" >> "$SUMMARY_FILE"
        fi
    done
done

cat >> "$SUMMARY_FILE" <<EOF

## Analysis

### Bandwidth Impact on Performance

Results show how different bandwidth constraints affect:
- **Convergence time**: Time for all nodes to receive updates
- **Per-update latency**: Individual message transmission time
- **Consistency**: System behavior across topology modes

### Key Findings

(Analysis will be visible after test completion)

EOF

echo ""
echo "======================================"
echo "Test suite complete!"
echo "======================================"
echo "Results saved to: $RESULTS_BASE_DIR/"
echo ""
echo "View comprehensive summary:"
echo "cat $RESULTS_BASE_DIR/COMPREHENSIVE_SUMMARY.md"
echo ""
