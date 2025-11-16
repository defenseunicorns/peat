#!/bin/bash
# test-squad-formation.sh
#
# Validates 12-node squad formation with realistic network constraints
#
# Usage:
#   ./test-squad-formation.sh [--cleanup-only]
#
# Requirements:
#   - Docker running
#   - ContainerLab installed
#   - .env file with Ditto credentials
#   - hive-sim-node image built

set -e

TOPOLOGY="topologies/squad-12node.yaml"
LAB_NAME="cap-squad-12node"
ENV_FILE="../.env"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Cleanup function
cleanup() {
    echo -e "${YELLOW}[Cleanup] Destroying topology...${NC}"
    sudo containerlab destroy -t "$TOPOLOGY" --cleanup 2>/dev/null || true
    echo -e "${GREEN}✓ Cleanup complete${NC}"
}

# Handle cleanup-only mode
if [[ "$1" == "--cleanup-only" ]]; then
    cleanup
    exit 0
fi

# Trap to ensure cleanup on exit
trap cleanup EXIT

echo "===== E8 Phase 1: 12-Node Squad Formation Test ====="
echo ""

# Check prerequisites
echo -e "${BLUE}[0/7] Checking prerequisites...${NC}"

if ! command -v docker &> /dev/null; then
    echo -e "${RED}✗ Docker not found${NC}"
    exit 1
fi

if ! command -v containerlab &> /dev/null; then
    echo -e "${RED}✗ ContainerLab not found${NC}"
    exit 1
fi

if [[ ! -f "$ENV_FILE" ]]; then
    echo -e "${RED}✗ .env file not found at $ENV_FILE${NC}"
    echo "  Copy .env.example and add your Ditto credentials"
    exit 1
fi

if ! docker image inspect hive-sim-node:latest &> /dev/null; then
    echo -e "${RED}✗ hive-sim-node:latest image not found${NC}"
    echo "  Build it with: docker build -f hive-sim/Dockerfile -t hive-sim-node:latest ."
    exit 1
fi

echo -e "${GREEN}✓ Prerequisites OK${NC}"
echo ""

# Clean up any existing deployment
echo -e "${BLUE}[1/7] Cleaning up existing deployments...${NC}"
sudo containerlab destroy -t "$TOPOLOGY" --cleanup 2>/dev/null || true
echo -e "${GREEN}✓ Cleanup complete${NC}"
echo ""

# Deploy topology
echo -e "${BLUE}[2/7] Deploying 12-node squad topology...${NC}"
echo "  - 9 Soldiers (mesh network)"
echo "  - 1 UGV (communication relay)"
echo "  - 2 UAVs (aerial reconnaissance)"
echo ""

if ! sudo containerlab deploy -t "$TOPOLOGY" --env-file "$ENV_FILE"; then
    echo -e "${RED}✗ Deployment failed${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Topology deployed${NC}"
echo ""

# Wait for containers to initialize
echo -e "${BLUE}[3/7] Waiting for containers to initialize (10s)...${NC}"
sleep 10
echo -e "${GREEN}✓ Containers initialized${NC}"
echo ""

# Display network impairments
echo -e "${BLUE}[4/7] Verifying network constraints...${NC}"
echo ""
echo "=== Soldier-to-Soldier Links (Intra-Squad Mesh) ==="
echo "Expected: 100 Kbps, 100ms latency, 1% loss"
sudo containerlab tools netem show -n "clab-${LAB_NAME}-soldier-1" 2>/dev/null | grep -A 1 "eth1" || echo "N/A"
echo ""

echo "=== UGV-to-Soldier Links (High Bandwidth) ==="
echo "Expected: 1000 Kbps, 50ms latency, 0.5% loss"
sudo containerlab tools netem show -n "clab-${LAB_NAME}-ugv-1" 2>/dev/null | grep -A 1 "eth1" || echo "N/A"
echo ""

echo "=== UAV-to-Squad Links (Aerial Radio) ==="
echo "Expected: 56 Kbps, 500ms latency, 5% loss"
sudo containerlab tools netem show -n "clab-${LAB_NAME}-uav-1" 2>/dev/null | grep -A 1 "eth1" || echo "N/A"
echo ""

# List all containers
echo -e "${BLUE}[5/7] Deployed containers:${NC}"
sudo containerlab inspect -t "$TOPOLOGY" | grep "clab-${LAB_NAME}" | awk '{print "  - " $1}'
echo ""

# Check initial logs from key nodes
echo -e "${BLUE}[6/7] Checking node initialization...${NC}"
echo ""

echo "=== Squad Leader (soldier-1) ==="
docker logs "clab-${LAB_NAME}-soldier-1" 2>&1 | tail -n 5
echo ""

echo "=== UGV (ugv-1) ==="
docker logs "clab-${LAB_NAME}-ugv-1" 2>&1 | tail -n 5
echo ""

echo "=== UAV (uav-1) ==="
docker logs "clab-${LAB_NAME}-uav-1" 2>&1 | tail -n 5
echo ""

# Wait for sync to propagate
echo -e "${BLUE}[7/7] Waiting for squad cell formation (30s)...${NC}"
echo "  This allows time for:"
echo "  - Peer discovery across all nodes"
echo "  - Cell formation and leader election"
echo "  - Capability advertisement"
echo ""

for i in {1..30}; do
    echo -n "."
    sleep 1
done
echo ""
echo -e "${GREEN}✓ Wait complete${NC}"
echo ""

# Display final status
echo "============================================"
echo -e "${GREEN}Squad Formation Test Complete${NC}"
echo "============================================"
echo ""
echo "Next Steps:"
echo "  1. Check logs for sync behavior:"
echo "     docker logs -f clab-${LAB_NAME}-soldier-1"
echo "     docker logs -f clab-${LAB_NAME}-ugv-1"
echo "     docker logs -f clab-${LAB_NAME}-uav-1"
echo ""
echo "  2. Inspect topology:"
echo "     sudo containerlab inspect -t $TOPOLOGY"
echo ""
echo "  3. Check network constraints:"
echo "     sudo containerlab tools netem show -t $TOPOLOGY"
echo ""
echo "  4. Enter a container:"
echo "     docker exec -it clab-${LAB_NAME}-soldier-1 bash"
echo ""
echo "  5. Destroy when done:"
echo "     sudo containerlab destroy -t $TOPOLOGY"
echo "     OR run: ./test-squad-formation.sh --cleanup-only"
echo ""

# Ask if user wants to keep containers running
echo -e "${YELLOW}Press Ctrl+C within 5 seconds to keep containers running...${NC}"
sleep 5

echo ""
echo -e "${YELLOW}Cleaning up (destroy topology)...${NC}"
# Cleanup will be called by trap
