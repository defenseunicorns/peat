#!/usr/bin/env bash
set -euo pipefail

#####################################################################
# Scaling Validation Test Script
#
# Tests Traditional Baseline at various node scales (96, 192, 384)
# with comprehensive resource monitoring and metrics collection.
#
# Usage: ./test-scaling-validation.sh <node_count>
# Example: ./test-scaling-validation.sh 192
#####################################################################

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
NODE_COUNT="${1:-96}"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_DIR="scaling-results-${NODE_COUNT}node-${TIMESTAMP}"
TEST_DURATION=60  # seconds
RESOURCE_SAMPLE_INTERVAL=5  # seconds

# Topology file mapping
case $NODE_COUNT in
    48)
        TOPOLOGY_FILE="topologies/traditional-battalion-48node.yaml"
        ;;
    96)
        TOPOLOGY_FILE="topologies/traditional-battalion-96node.yaml"
        ;;
    192)
        TOPOLOGY_FILE="topologies/traditional-battalion-192node.yaml"
        ;;
    384)
        TOPOLOGY_FILE="topologies/traditional-battalion-384node.yaml"
        ;;
    500)
        TOPOLOGY_FILE="topologies/traditional-battalion-500node.yaml"
        ;;
    750)
        TOPOLOGY_FILE="topologies/traditional-battalion-750node.yaml"
        ;;
    1000)
        TOPOLOGY_FILE="topologies/traditional-battalion-1000node.yaml"
        ;;
    1500)
        TOPOLOGY_FILE="topologies/traditional-battalion-1500node.yaml"
        ;;
    2000)
        TOPOLOGY_FILE="topologies/traditional-battalion-2000node.yaml"
        ;;
    *)
        echo -e "${RED}❌ Error: Unsupported node count: $NODE_COUNT${NC}"
        echo "Supported: 48, 96, 192, 384, 500, 750, 1000, 1500, 2000"
        exit 1
        ;;
esac

# Validate topology file exists
if [ ! -f "$TOPOLOGY_FILE" ]; then
    echo -e "${RED}❌ Error: Topology file not found: $TOPOLOGY_FILE${NC}"
    exit 1
fi

# Create results directory
mkdir -p "$RESULTS_DIR"

echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Scaling Validation - ${NODE_COUNT} Nodes${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${GREEN}📋 Configuration:${NC}"
echo "   • Node count: $NODE_COUNT"
echo "   • Topology: $TOPOLOGY_FILE"
echo "   • Test duration: ${TEST_DURATION}s"
echo "   • Results directory: $RESULTS_DIR"
echo ""

#####################################################################
# Resource Monitoring Functions
#####################################################################

start_resource_monitoring() {
    echo -e "${GREEN}📊 Starting resource monitoring...${NC}"

    # Background process to collect system resources every 5 seconds
    (
        echo "timestamp,cpu_count,mem_total_gb,mem_used_gb,mem_free_gb,mem_avail_gb,mem_percent,docker_containers" \
            > "$RESULTS_DIR/system-resources.csv"

        while true; do
            TIMESTAMP=$(date +%s)
            CPU_COUNT=$(nproc)

            # Memory stats (in GB)
            MEM_STATS=$(free -g | grep Mem:)
            MEM_TOTAL=$(echo "$MEM_STATS" | awk '{print $2}')
            MEM_USED=$(echo "$MEM_STATS" | awk '{print $3}')
            MEM_FREE=$(echo "$MEM_STATS" | awk '{print $4}')
            MEM_AVAIL=$(echo "$MEM_STATS" | awk '{print $7}')
            MEM_PERCENT=$(free | grep Mem: | awk '{printf "%.1f", $3/$2 * 100.0}')

            # Container count
            CONTAINER_COUNT=$(docker ps -q 2>/dev/null | wc -l)

            echo "$TIMESTAMP,$CPU_COUNT,$MEM_TOTAL,$MEM_USED,$MEM_FREE,$MEM_AVAIL,$MEM_PERCENT,$CONTAINER_COUNT" \
                >> "$RESULTS_DIR/system-resources.csv"

            sleep $RESOURCE_SAMPLE_INTERVAL
        done
    ) &

    RESOURCE_MONITOR_PID=$!
    echo "   Resource monitor PID: $RESOURCE_MONITOR_PID"
}

stop_resource_monitoring() {
    if [ -n "${RESOURCE_MONITOR_PID:-}" ]; then
        echo -e "${GREEN}📊 Stopping resource monitoring...${NC}"
        kill $RESOURCE_MONITOR_PID 2>/dev/null || true
        wait $RESOURCE_MONITOR_PID 2>/dev/null || true
    fi
}

#####################################################################
# Cleanup Function
#####################################################################

cleanup() {
    EXIT_CODE=$?

    echo ""
    echo -e "${YELLOW}🧹 Cleaning up...${NC}"

    # Stop resource monitoring
    stop_resource_monitoring

    # Destroy topology with force cleanup on timeout
    echo "   Destroying topology..."
    timeout 30 containerlab destroy --all --cleanup 2>/dev/null || {
        echo -e "${YELLOW}   ⚠️  Normal destroy timed out, forcing cleanup...${NC}"
        docker ps -a --filter "name=clab-traditional-battalion" -q | xargs -r docker rm -f 2>/dev/null || true
    }

    if [ $EXIT_CODE -eq 0 ]; then
        echo -e "${GREEN}✅ Test completed successfully${NC}"
        echo -e "${GREEN}📂 Results saved to: $RESULTS_DIR${NC}"
    else
        echo -e "${RED}❌ Test failed with exit code: $EXIT_CODE${NC}"
    fi

    exit $EXIT_CODE
}

trap cleanup EXIT INT TERM

#####################################################################
# Main Test Flow
#####################################################################

# 1. Check system resources before deployment
echo -e "${GREEN}💻 Pre-deployment system check:${NC}"
free -h | grep -E "Mem:|Swap:"
echo "   CPU cores: $(nproc)"
echo ""

# 2. Start resource monitoring
start_resource_monitoring

# 3. Deploy topology
echo -e "${GREEN}🚀 Deploying ${NODE_COUNT}-node topology...${NC}"
echo "   Using max-workers to avoid timeout issues"
DEPLOY_START=$(date +%s)

# Use --max-workers to control concurrent deployment
# For large topologies, limit workers to avoid overwhelming the system
if [ "$NODE_COUNT" -ge 384 ]; then
    MAX_WORKERS=16
elif [ "$NODE_COUNT" -ge 192 ]; then
    MAX_WORKERS=24
else
    MAX_WORKERS=32
fi

echo "   Max workers: $MAX_WORKERS"

# For large deployments, increase containerlab's timeout for individual operations
CLAB_TIMEOUT="5m"
echo "   Containerlab timeout: $CLAB_TIMEOUT"

if ! timeout 600 containerlab deploy -t "$TOPOLOGY_FILE" --max-workers "$MAX_WORKERS" --timeout "$CLAB_TIMEOUT"; then
    echo -e "${RED}❌ Deployment failed or timed out${NC}"
    exit 1
fi

DEPLOY_END=$(date +%s)
DEPLOY_TIME=$((DEPLOY_END - DEPLOY_START))
echo "   ✅ Deployment completed in ${DEPLOY_TIME}s"
echo "$DEPLOY_TIME" > "$RESULTS_DIR/deployment-time.txt"

# 4. Wait for containers to stabilize
echo ""
echo -e "${GREEN}⏳ Waiting 10s for containers to stabilize...${NC}"
sleep 10

# 5. Verify all containers are running
echo ""
echo -e "${GREEN}🔍 Verifying container health...${NC}"
EXPECTED_CONTAINERS=$NODE_COUNT
ACTUAL_CONTAINERS=$(docker ps --filter "name=clab-traditional-battalion-${NODE_COUNT}node" -q | wc -l)

echo "   Expected containers: $EXPECTED_CONTAINERS"
echo "   Running containers: $ACTUAL_CONTAINERS"

if [ "$ACTUAL_CONTAINERS" -ne "$EXPECTED_CONTAINERS" ]; then
    echo -e "${YELLOW}⚠️  Warning: Container count mismatch${NC}"
fi

echo "$ACTUAL_CONTAINERS" > "$RESULTS_DIR/container-count.txt"

# 6. Collect baseline Docker stats
echo ""
echo -e "${GREEN}📊 Collecting Docker resource stats...${NC}"
docker stats --no-stream --format "table {{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}\t{{.NetIO}}" \
    --filter "name=clab-traditional-battalion-${NODE_COUNT}node" \
    > "$RESULTS_DIR/docker-stats-initial.txt" 2>/dev/null || echo "Failed to collect Docker stats"

# 7. Run test for specified duration
echo ""
echo -e "${GREEN}🧪 Running test for ${TEST_DURATION}s...${NC}"
echo "   Monitoring container logs for metrics..."

TEST_START=$(date +%s)

# Collect logs from a sample of containers (first 5 soldiers + HQ)
SAMPLE_CONTAINERS=$(docker ps --filter "name=clab-traditional-battalion-${NODE_COUNT}node" --format "{{.Names}}" | head -6)

for container in $SAMPLE_CONTAINERS; do
    echo "   Sampling logs from: $container"
done

# Wait for test duration
sleep "$TEST_DURATION"

TEST_END=$(date +%s)
TEST_TIME=$((TEST_END - TEST_START))

# 8. Collect final Docker stats
echo ""
echo -e "${GREEN}📊 Collecting final Docker resource stats...${NC}"
docker stats --no-stream --format "table {{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}\t{{.NetIO}}" \
    --filter "name=clab-traditional-battalion-${NODE_COUNT}node" \
    > "$RESULTS_DIR/docker-stats-final.txt" 2>/dev/null || echo "Failed to collect Docker stats"

# 9. Collect container logs for metrics analysis
echo ""
echo -e "${GREEN}📝 Collecting container logs...${NC}"

# Collect logs from all containers (JSONL metrics)
mkdir -p "$RESULTS_DIR/logs"

CONTAINER_LIST=$(docker ps --filter "name=clab-traditional-battalion-${NODE_COUNT}node" --format "{{.Names}}")
CONTAINER_ARRAY=($CONTAINER_LIST)
TOTAL_CONTAINERS=${#CONTAINER_ARRAY[@]}

echo "   Collecting logs from $TOTAL_CONTAINERS containers..."

# Use parallel collection with limit to avoid overwhelming system
BATCH_SIZE=20
for ((i=0; i<$TOTAL_CONTAINERS; i+=$BATCH_SIZE)); do
    BATCH_END=$((i + BATCH_SIZE))
    if [ $BATCH_END -gt $TOTAL_CONTAINERS ]; then
        BATCH_END=$TOTAL_CONTAINERS
    fi

    echo "   Processing containers $i-$BATCH_END..."

    for ((j=i; j<BATCH_END; j++)); do
        container="${CONTAINER_ARRAY[$j]}"
        docker logs "$container" > "$RESULTS_DIR/logs/${container}.log" 2>&1 &
    done

    # Wait for this batch to complete
    wait
done

echo "   ✅ Log collection complete"

# 10. Generate summary
echo ""
echo -e "${GREEN}📋 Generating test summary...${NC}"

cat > "$RESULTS_DIR/test-summary.txt" <<EOF
Scaling Validation Test Summary
================================

Test Configuration:
  • Node count: $NODE_COUNT
  • Topology file: $TOPOLOGY_FILE
  • Test duration: ${TEST_DURATION}s
  • Timestamp: $TIMESTAMP

Deployment:
  • Deployment time: ${DEPLOY_TIME}s
  • Max workers: $MAX_WORKERS
  • Expected containers: $EXPECTED_CONTAINERS
  • Actual containers: $ACTUAL_CONTAINERS
  • Status: $([ "$ACTUAL_CONTAINERS" -eq "$EXPECTED_CONTAINERS" ] && echo "✅ SUCCESS" || echo "⚠️  PARTIAL")

System Resources (at test end):
$(free -h | grep -E "Mem:|Swap:")
  • CPU cores: $(nproc)
  • Active containers: $(docker ps -q | wc -l)

Results Location:
  • Directory: $RESULTS_DIR
  • System resources: system-resources.csv
  • Docker stats: docker-stats-*.txt
  • Container logs: logs/

EOF

cat "$RESULTS_DIR/test-summary.txt"

echo ""
echo -e "${GREEN}✅ Test completed successfully!${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
