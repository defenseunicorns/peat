#!/bin/bash
# Phase 3A Platoon (24-node) Validation Smoke Test
#
# Purpose: Quick validation that 24-node platoon topologies work
# Runtime: ~2 minutes per topology
# Usage: ./validate-platoon-24node.sh [topology-name]
#
# If no topology specified, tests platoon-24node-client-server (simplest)

set -e

cd "$(dirname "$0")"

TOPOLOGY="${1:-platoon-24node-client-server}"
TOPOLOGY_FILE="topologies/${TOPOLOGY}.yaml"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║   Phase 3A: 24-Node Platoon Validation Smoke Test        ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Topology: $TOPOLOGY"
echo "File: $TOPOLOGY_FILE"
echo ""

# Verify topology file exists
if [ ! -f "$TOPOLOGY_FILE" ]; then
    echo "❌ Error: Topology file not found: $TOPOLOGY_FILE"
    exit 1
fi

echo "✓ Topology file found"
echo ""

# Create validation results directory
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="validation-platoon-24node-$TIMESTAMP"
mkdir -p "$RESULTS_DIR"

echo "📁 Results directory: $RESULTS_DIR"
echo ""

# Load environment variables
set -a
source ../.env 2>/dev/null || true
set +a

if [ -z "$DITTO_APP_ID" ]; then
    echo "⚠️  Warning: DITTO_APP_ID not set (may cause issues)"
fi

# Clean up any existing deployment
echo "🧹 Cleaning up any existing deployment..."
containerlab destroy -t "$TOPOLOGY_FILE" --cleanup > /dev/null 2>&1 || true
sleep 2
echo ""

# Deploy topology
echo "═══════════════════════════════════════════════════════════"
echo "Deploying 24-node platoon topology..."
echo "═══════════════════════════════════════════════════════════"
echo ""

deploy_start=$(date +%s)
containerlab deploy -t "$TOPOLOGY_FILE" > "$RESULTS_DIR/deploy.log" 2>&1
deploy_time=$(($(date +%s) - deploy_start))

echo "✓ Deployment complete in ${deploy_time}s"
echo ""

# Extract lab name
lab_name=$(grep "name:" "$TOPOLOGY_FILE" | head -1 | awk '{print $2}')
echo "Lab name: $lab_name"
echo ""

# Verify node count
echo "Verifying node count..."
node_count=$(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}" | wc -l)
echo "  Nodes running: $node_count / 24"

if [ "$node_count" -ne 24 ]; then
    echo "❌ Error: Expected 24 nodes, got $node_count"
    echo ""
    echo "Cleaning up..."
    containerlab destroy -t "$TOPOLOGY_FILE" --cleanup > /dev/null 2>&1 || true
    exit 1
fi

echo "✓ All 24 nodes deployed successfully"
echo ""

# Wait for nodes to initialize and sync
echo "⏳ Waiting for node initialization and sync (60s)..."
echo "   Target convergence: <60s (2× squad baseline of ~26s)"
sleep 60
echo ""

# Check sync status
echo "═══════════════════════════════════════════════════════════"
echo "Checking sync status..."
echo "═══════════════════════════════════════════════════════════"
echo ""

synced_count=0
failed_count=0
unknown_count=0

docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}" | while read container; do
    node_name=$(echo "$container" | sed "s/clab-${lab_name}-//")

    if docker logs "$container" 2>&1 | grep -q "POC SUCCESS"; then
        status="SYNCED ✓"
        synced_count=$((synced_count + 1))
    elif docker logs "$container" 2>&1 | grep -q "POC FAILED"; then
        status="FAILED ✗"
        failed_count=$((failed_count + 1))
    else
        status="UNKNOWN ?"
        unknown_count=$((unknown_count + 1))
    fi

    echo "  $node_name: $status"
done | tee "$RESULTS_DIR/sync_status.txt"

echo ""

# Count results
synced_count=$(grep -c "SYNCED ✓" "$RESULTS_DIR/sync_status.txt" || echo "0")
failed_count=$(grep -c "FAILED ✗" "$RESULTS_DIR/sync_status.txt" || echo "0")
unknown_count=$(grep -c "UNKNOWN ?" "$RESULTS_DIR/sync_status.txt" || echo "0")

echo "Summary:"
echo "  Synced: $synced_count / 24"
echo "  Failed: $failed_count"
echo "  Unknown: $unknown_count"
echo ""

# Collect logs from all containers
echo "═══════════════════════════════════════════════════════════"
echo "Collecting container logs..."
echo "═══════════════════════════════════════════════════════════"
echo ""

for container in $(docker ps --filter "name=clab-${lab_name}-" --format "{{.Names}}"); do
    node_name=$(echo "$container" | sed "s/clab-${lab_name}-//")
    docker logs "$container" > "$RESULTS_DIR/${node_name}.log" 2>&1
    echo "  ✓ $node_name"
done

echo ""
echo "✓ Logs collected: $RESULTS_DIR/*.log"
echo ""

# Analyze performance metrics (if analyze_metrics.py exists)
if [ -f "analyze_metrics.py" ]; then
    echo "═══════════════════════════════════════════════════════════"
    echo "Analyzing performance metrics..."
    echo "═══════════════════════════════════════════════════════════"
    echo ""

    log_files=""
    for log_file in "$RESULTS_DIR"/*.log; do
        [ -f "$log_file" ] && log_files="$log_files $log_file"
    done

    if python3 analyze_metrics.py $log_files > "$RESULTS_DIR/metrics.json" 2>/dev/null; then
        echo "✓ Metrics analysis complete"

        convergence_ms=$(jq -r '.convergence.convergence_time_ms // 0' "$RESULTS_DIR/metrics.json" 2>/dev/null | awk '{printf "%.2f", $1}')
        latency_mean=$(jq -r '.latency.mean // 0' "$RESULTS_DIR/metrics.json" 2>/dev/null | awk '{printf "%.2f", $1}')
        latency_p90=$(jq -r '.latency.p90 // 0' "$RESULTS_DIR/metrics.json" 2>/dev/null | awk '{printf "%.2f", $1}')

        echo ""
        echo "Performance Metrics:"
        echo "  Convergence time: ${convergence_ms}ms"
        echo "  Mean latency: ${latency_mean}ms"
        echo "  P90 latency: ${latency_p90}ms"

        # Check against Phase 3A success criteria (<60s = <60000ms)
        if [ $(echo "$convergence_ms < 60000" | bc -l) -eq 1 ]; then
            echo "  ✓ Convergence < 60s target (Phase 3A criteria)"
        else
            echo "  ⚠️  Convergence exceeds 60s target"
        fi
    else
        echo "⚠️  Metrics analysis skipped (python3 or jq not available)"
    fi
    echo ""
fi

# Clean up
echo "═══════════════════════════════════════════════════════════"
echo "Cleaning up..."
echo "═══════════════════════════════════════════════════════════"
echo ""

containerlab destroy -t "$TOPOLOGY_FILE" --cleanup > /dev/null 2>&1 || {
    echo "⚠️  Destroy failed, forcing cleanup..."
    docker rm -f $(docker ps -a -q --filter "name=clab-${lab_name}-" 2>/dev/null) 2>/dev/null || true
}

echo "✓ Cleanup complete"
echo ""

# Final summary
echo "╔════════════════════════════════════════════════════════════╗"
echo "║   Validation Summary                                      ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Topology: $TOPOLOGY"
echo "Nodes: $node_count / 24"
echo "Synced: $synced_count / 24"
echo "Failed: $failed_count"
echo "Deploy time: ${deploy_time}s"
echo ""
echo "Results: $RESULTS_DIR/"
echo "Logs: $RESULTS_DIR/*.log"
echo "Metrics: $RESULTS_DIR/metrics.json"
echo ""

# Exit with success/failure based on sync count
if [ "$synced_count" -ge 20 ]; then
    echo "✅ VALIDATION PASSED (${synced_count}/24 nodes synced)"
    exit 0
elif [ "$synced_count" -ge 12 ]; then
    echo "⚠️  VALIDATION PARTIAL (${synced_count}/24 nodes synced)"
    exit 0
else
    echo "❌ VALIDATION FAILED (only ${synced_count}/24 nodes synced)"
    exit 1
fi
