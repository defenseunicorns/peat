#!/bin/bash
# Test script for Traditional IoT Baseline (NO CRDT, periodic full messages)
# Runs 2-node and 12-node traditional baseline tests

set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Traditional IoT Baseline Testing ===${NC}"
echo "Testing traditional event-driven architecture (NO CRDT)"
echo ""

# Check for required environment variables
if [ ! -f .env ]; then
    echo -e "${RED}ERROR: .env file not found${NC}"
    echo "Please create .env with DITTO credentials (required by entrypoint even if not used)"
    exit 1
fi

# Source environment variables
set -a
source .env
set +a

if [ -z "$DITTO_APP_ID" ]; then
    echo -e "${RED}ERROR: DITTO_APP_ID not set in .env${NC}"
    exit 1
fi

# Function to run a test scenario
run_test() {
    local topology_file=$1
    local test_name=$2
    local duration=$3

    echo -e "\n${GREEN}━━━ Testing: $test_name ━━━${NC}"
    echo "Topology: $topology_file"
    echo "Duration: ${duration}s"
    echo ""

    # Clean up any existing deployment
    echo -e "${YELLOW}Cleaning up any existing deployment...${NC}"
    containerlab destroy -t "$topology_file" --cleanup 2>/dev/null || true
    sleep 2

    # Deploy topology
    echo -e "${YELLOW}Deploying topology...${NC}"
    containerlab deploy -t "$topology_file"

    # Get container names
    local topology_name=$(grep "^name:" "$topology_file" | awk '{print $2}')
    echo -e "${YELLOW}Topology name: $topology_name${NC}"

    # Wait for containers to start
    echo -e "${YELLOW}Waiting 5s for containers to initialize...${NC}"
    sleep 5

    # Get first node name to monitor
    local first_node=$(containerlab inspect -t "$topology_file" | grep "clab-$topology_name" | head -1 | awk '{print $2}')
    echo -e "${YELLOW}Monitoring node: $first_node${NC}"

    # Monitor logs
    echo -e "${YELLOW}Monitoring for ${duration}s...${NC}"
    timeout $duration docker logs -f "$first_node" 2>&1 | grep -E "(TRADITIONAL|Full state|POC|bytes|documents)" || true

    # Show final metrics from all nodes
    echo -e "\n${YELLOW}Collecting final metrics from all nodes...${NC}"
    for container in $(containerlab inspect -t "$topology_file" | grep "clab-$topology_name" | awk '{print $2}'); do
        echo -e "\n${BLUE}─── Metrics from $container ───${NC}"
        docker logs "$container" 2>&1 | tail -20 | grep -E "(POC|bytes|documents|sequence)" || echo "No metrics found"
    done

    # Destroy topology
    echo -e "\n${YELLOW}Destroying topology...${NC}"
    containerlab destroy -t "$topology_file" --cleanup

    echo -e "${GREEN}✓ Test completed: $test_name${NC}"
}

# Test 1: 2-Node Traditional Baseline
run_test \
    "topologies/traditional-2node.yaml" \
    "2-Node Traditional Baseline (Client-Server)" \
    30

# Test 2: 12-Node Client-Server
run_test \
    "topologies/traditional-squad-client-server.yaml" \
    "12-Node Squad - Traditional Client-Server" \
    60

# Test 3: 12-Node Hub-Spoke
run_test \
    "topologies/traditional-squad-hub-spoke.yaml" \
    "12-Node Squad - Traditional Hub-Spoke" \
    60

echo -e "\n${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}All Traditional Baseline Tests Completed!${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "Summary:"
echo "  ✓ 2-Node Traditional Baseline"
echo "  ✓ 12-Node Client-Server"
echo "  ✓ 12-Node Hub-Spoke"
echo ""
echo "Next Steps:"
echo "  1. Compare bandwidth usage vs CAP Full (CRDT overhead)"
echo "  2. Compare bandwidth usage vs CAP Differential (CAP filtering benefit)"
echo "  3. Analyze convergence time differences"
echo ""
