#!/bin/bash
# Common test functions for peat-sim lab experiments
#
# Sourced by:
# - test-producer-only.sh (Lab 1)
# - test-traditional-baseline.sh (Lab 2)
# - test-lab3b-peat-mesh.sh (Lab 3b)
# - test-lab4-hierarchical-peat-crdt.sh (Lab 4)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Validate that required tools are available
validate_environment() {
    echo "Validating environment..."

    # Check Docker
    if ! command -v docker &> /dev/null; then
        echo -e "${RED}ERROR: docker not found${NC}"
        exit 1
    fi

    # Check containerlab
    if ! command -v containerlab &> /dev/null; then
        echo -e "${RED}ERROR: containerlab not found${NC}"
        exit 1
    fi

    # Check Python3
    if ! command -v python3 &> /dev/null; then
        echo -e "${RED}ERROR: python3 not found${NC}"
        exit 1
    fi

    # Check Docker image exists
    if ! docker images | grep -q "peat-sim-node"; then
        echo -e "${RED}ERROR: peat-sim-node:latest image not found${NC}"
        echo "Run: docker build -t peat-sim-node:latest -f peat-sim/Dockerfile ."
        exit 1
    fi

    # Check for Ditto/PEAT credentials if needed
    # Use set -a to auto-export all variables (required for containerlab to pass them to containers)
    if [ -f "../.env" ]; then
        set -a
        source ../.env
        set +a
    fi

    # Verify credentials are available
    if [ -z "${DITTO_APP_ID:-}" ] && [ -z "${PEAT_APP_ID:-}" ]; then
        echo -e "${YELLOW}WARNING: No DITTO_APP_ID or PEAT_APP_ID found in environment${NC}"
        echo -e "${YELLOW}         Ditto backend tests may fail to initialize${NC}"
    fi

    echo -e "${GREEN}Environment validated${NC}"
    echo ""
}

# Apply bandwidth constraint to all containers matching a pattern
apply_bandwidth_constraint() {
    local TOPO_NAME="$1"
    local BANDWIDTH="$2"

    # Convert bandwidth to Kbps for netem
    local RATE_KBPS
    case "$BANDWIDTH" in
        "1gbps")    RATE_KBPS=1048576 ;;
        "100mbps")  RATE_KBPS=102400 ;;
        "1mbps")    RATE_KBPS=1024 ;;
        "256kbps")  RATE_KBPS=256 ;;
        *)          RATE_KBPS=1048576 ;;  # Default to 1Gbps
    esac

    # Apply netem rate limit using containerlab tools
    containerlab tools netem set -n "$TOPO_NAME" --rate "${RATE_KBPS}kbit" 2>/dev/null || true
}

# Cleanup any running containers from a topology
cleanup_topology() {
    local TOPO_FILE="$1"
    containerlab destroy -t "$TOPO_FILE" --cleanup 2>/dev/null || true
}

# Extract METRICS lines from container logs
extract_metrics() {
    local CONTAINER="$1"
    local OUTPUT_FILE="$2"

    docker logs "$CONTAINER" 2>&1 | grep "^METRICS:" | sed 's/^METRICS: //' > "$OUTPUT_FILE"
}

# Calculate percentiles from a file of numbers
percentile() {
    local FILE="$1"
    local P="$2"  # 50, 90, 95, 99

    if [ ! -s "$FILE" ]; then
        echo "0"
        return
    fi

    local COUNT=$(wc -l < "$FILE")
    local INDEX=$(echo "scale=0; ($COUNT * $P / 100)" | bc)

    if [ "$INDEX" -lt 1 ]; then
        INDEX=1
    fi

    sort -n "$FILE" | sed -n "${INDEX}p"
}

# Log with timestamp
log_info() {
    echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[$(date +%H:%M:%S)] ✅ $1${NC}"
}

log_error() {
    echo -e "${RED}[$(date +%H:%M:%S)] ❌ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠️  $1${NC}"
}
