#!/bin/bash
# Run complete three-way baseline comparison
# Traditional IoT vs CAP Full vs CAP Differential

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Three-Way Baseline Comparison                            ║${NC}"
echo -e "${BLUE}║  Traditional IoT vs CAP Full vs CAP Differential          ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Create results directory
RESULTS_DIR="baseline-comparison-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$RESULTS_DIR"
echo -e "${GREEN}Results directory: $RESULTS_DIR${NC}"
echo ""

# Check for .env file
if [ ! -f .env ]; then
    echo -e "${RED}ERROR: .env file not found${NC}"
    echo "Please create .env with DITTO credentials"
    exit 1
fi

# Source environment variables
set -a
source .env
set +a

# Robust cleanup function with timeout and fallback
cleanup_topology() {
    local topology_file=$1

    # Extract topology name from file
    local topology_name=$(grep "^name:" "$topology_file" | awk '{print $2}')

    echo -e "${YELLOW}Destroying topology...${NC}"

    # Try containerlab destroy with 30 second timeout
    if timeout 30 containerlab destroy -t "$topology_file" --cleanup 2>/dev/null; then
        return 0
    fi

    # If timeout or failure, force cleanup containers
    echo -e "${YELLOW}⚠️  Containerlab destroy timed out, forcing cleanup...${NC}"

    # Get all containers for this topology
    local containers=$(docker ps -a --filter "name=clab-${topology_name}-" --format "{{.Names}}")

    if [ -n "$containers" ]; then
        echo "$containers" | xargs -r docker rm -f > /dev/null 2>&1 || true
    fi

    # Clean up network if it exists
    docker network rm "clab-${topology_name}" 2>/dev/null || true
}

# Function to run a single test configuration
run_test() {
    local arch=$1
    local topology_file=$2
    local test_name=$3
    local duration=$4
    local use_traditional=$5
    local cap_filter=$6

    echo -e "\n${GREEN}━━━ Testing: $test_name ━━━${NC}"
    echo "Architecture: $arch"
    echo "Topology: $topology_file"
    echo "Duration: ${duration}s"
    echo ""

    # Clean up any existing deployment
    echo -e "${YELLOW}Cleaning up...${NC}"
    containerlab destroy -t "$topology_file" --cleanup 2>/dev/null || true
    sleep 2

    # For CAP tests, we need to modify the topology to add CAP_FILTER_ENABLED if needed
    local deploy_file="$topology_file"
    if [ "$use_traditional" = "false" ] && [ "$cap_filter" = "true" ]; then
        # Create temporary topology with CAP filtering enabled
        deploy_file="${topology_file%.yaml}-cap-differential-temp.yaml"
        python3 -c "
import yaml
import sys

with open('$topology_file') as f:
    topo = yaml.safe_load(f)

# Add CAP_FILTER_ENABLED to all nodes
for node_name, node_config in topo['topology']['nodes'].items():
    if 'env' not in node_config:
        node_config['env'] = {}
    node_config['env']['CAP_FILTER_ENABLED'] = 'true'

with open('$deploy_file', 'w') as f:
    yaml.dump(topo, f, default_flow_style=False)
" 2>/dev/null || {
            echo -e "${RED}ERROR: Failed to create CAP Differential topology${NC}"
            echo "Install PyYAML: pip3 install pyyaml"
            exit 1
        }
    fi

    # Deploy topology
    echo -e "${YELLOW}Deploying...${NC}"
    containerlab deploy -t "$deploy_file"

    # Get topology name
    local topology_name=$(grep "^name:" "$deploy_file" | awk '{print $2}')
    echo -e "${YELLOW}Topology: $topology_name${NC}"

    # Wait for initialization
    echo -e "${YELLOW}Waiting 5s for initialization...${NC}"
    sleep 5

    # Collect logs from all nodes
    echo -e "${YELLOW}Running test for ${duration}s...${NC}"
    sleep $duration

    # Save logs
    echo -e "${YELLOW}Collecting logs...${NC}"
    local log_file="$RESULTS_DIR/${test_name}.log"
    for container in $(containerlab inspect -t "$deploy_file" | grep "clab-$topology_name" | awk '{print $2}'); do
        echo "=== $container ===" >> "$log_file"
        docker logs "$container" 2>&1 >> "$log_file"
        echo "" >> "$log_file"
    done

    # Extract metrics
    grep "METRICS:" "$log_file" > "$RESULTS_DIR/${test_name}-metrics.json" 2>/dev/null || echo "No metrics found" > "$RESULTS_DIR/${test_name}-metrics.json"

    # Cleanup with timeout protection
    cleanup_topology "$deploy_file"

    # Remove temporary file if created
    if [ "$deploy_file" != "$topology_file" ]; then
        rm -f "$deploy_file"
    fi

    echo -e "${GREEN}✓ Test completed: $test_name${NC}"
}

# ============================================
# 1. TRADITIONAL IOT BASELINE
# ============================================

echo -e "\n${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}PHASE 1: Traditional IoT Baseline (NO CRDT)${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"

run_test "Traditional IoT" "topologies/traditional-2node.yaml" \
    "1-traditional-2node" 30 "true" "false"

run_test "Traditional IoT" "topologies/traditional-squad-client-server.yaml" \
    "2-traditional-12node-client-server" 60 "true" "false"

run_test "Traditional IoT" "topologies/traditional-squad-hub-spoke.yaml" \
    "3-traditional-12node-hub-spoke" 60 "true" "false"

# ============================================
# 2. CAP FULL REPLICATION (CRDT without filtering)
# ============================================

echo -e "\n${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}PHASE 2: CAP Full Replication (CRDT without filtering)${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"

run_test "CAP Full" "topologies/poc-2node.yaml" \
    "4-cap-full-2node" 30 "false" "false"

run_test "CAP Full" "topologies/squad-12node-client-server.yaml" \
    "5-cap-full-12node-client-server" 60 "false" "false"

run_test "CAP Full" "topologies/squad-12node-hub-spoke.yaml" \
    "6-cap-full-12node-hub-spoke" 60 "false" "false"

# ============================================
# 3. CAP DIFFERENTIAL FILTERING (CRDT + capability filtering)
# ============================================

echo -e "\n${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}PHASE 3: CAP Differential (CRDT + Capability Filtering)${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"

run_test "CAP Differential" "topologies/poc-2node.yaml" \
    "7-cap-differential-2node" 30 "false" "true"

run_test "CAP Differential" "topologies/squad-12node-client-server.yaml" \
    "8-cap-differential-12node-client-server" 60 "false" "true"

run_test "CAP Differential" "topologies/squad-12node-hub-spoke.yaml" \
    "9-cap-differential-12node-hub-spoke" 60 "false" "true"

# ============================================
# GENERATE COMPARISON REPORT
# ============================================

echo -e "\n${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}Generating Comparison Report${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"

# Combine all traditional metrics
cat "$RESULTS_DIR"/1-traditional-*-metrics.json \
    "$RESULTS_DIR"/2-traditional-*-metrics.json \
    "$RESULTS_DIR"/3-traditional-*-metrics.json \
    > "$RESULTS_DIR/traditional-all-metrics.json" 2>/dev/null || true

# Combine all CAP Full metrics
cat "$RESULTS_DIR"/4-cap-full-*-metrics.json \
    "$RESULTS_DIR"/5-cap-full-*-metrics.json \
    "$RESULTS_DIR"/6-cap-full-*-metrics.json \
    > "$RESULTS_DIR/cap-full-all-metrics.json" 2>/dev/null || true

# Combine all CAP Differential metrics
cat "$RESULTS_DIR"/7-cap-differential-*-metrics.json \
    "$RESULTS_DIR"/8-cap-differential-*-metrics.json \
    "$RESULTS_DIR"/9-cap-differential-*-metrics.json \
    > "$RESULTS_DIR/cap-differential-all-metrics.json" 2>/dev/null || true

# Generate comparison report
python3 analyze-three-way-comparison.py \
    "$RESULTS_DIR/traditional-all-metrics.json" \
    "$RESULTS_DIR/cap-full-all-metrics.json" \
    "$RESULTS_DIR/cap-differential-all-metrics.json" \
    > "$RESULTS_DIR/COMPARISON-REPORT.txt" 2>&1 || {
        echo -e "${YELLOW}Warning: Comparison report generation had issues${NC}"
        echo "Check $RESULTS_DIR/COMPARISON-REPORT.txt for details"
    }

echo ""
echo -e "${GREEN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║  Three-Way Baseline Comparison COMPLETE!                  ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${GREEN}Results saved to: $RESULTS_DIR${NC}"
echo ""
echo "Files:"
echo "  • *-metrics.json - Raw JSON metrics for each test"
echo "  • *-all-metrics.json - Combined metrics by architecture"
echo "  • *.log - Full logs from each test"
echo "  • COMPARISON-REPORT.txt - Three-way comparison analysis"
echo ""
echo "View report:"
echo "  cat $RESULTS_DIR/COMPARISON-REPORT.txt"
echo ""
