#!/bin/bash
# Test Traditional IoT Baseline with different bandwidth constraints
# NO CRDT - Periodic full-state messaging architecture
#
# Bandwidth levels:
# - 100Mbps (unconstrained baseline)
# - 10Mbps (typical WiFi/LTE)
# - 1Mbps (constrained wireless)
# - 256Kbps (tactical radio)

set -e

cd "$(dirname "$0")"

echo "======================================"
echo "Traditional IoT Bandwidth Testing"
echo "NO CRDT - Periodic Full Messages"
echo "======================================"
echo ""

# Load environment variables
set -a
source ../.env 2>/dev/null || true
set +a

# Create results directory
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RESULTS_BASE_DIR="test-results-traditional-bandwidth-$TIMESTAMP"
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

# Test modes - using traditional baseline topologies
MODES=(
    "mode1-client-server:topologies/traditional-squad-client-server.yaml"
    "mode2-hub-spoke:topologies/traditional-squad-hub-spoke.yaml"
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
        containerlab tools netem delete -n "$container" -i eth0 2>/dev/null || true
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
        echo "  Deploying topology..."
        containerlab deploy -t "$topology_file" > "$BW_RESULTS_DIR/${mode_name}.deploy.log" 2>&1

        # Extract topology name from file
        topology_name=$(grep "^name:" "$topology_file" | awk '{print $2}')

        # Apply bandwidth constraint
        apply_bandwidth_constraint "$bw_rate" "$topology_name"

        # Wait for initialization
        echo "  Waiting 10s for initialization..."
        sleep 10

        # Run test for 60 seconds
        echo "  Running test for 60s..."
        sleep 60

        # Collect logs from all containers
        echo "  Collecting logs..."
        for container in $(docker ps --filter "name=clab-${topology_name}-" --format "{{.Names}}"); do
            echo "=== $container ===" >> "$BW_RESULTS_DIR/${mode_name}.log"
            docker logs "$container" 2>&1 >> "$BW_RESULTS_DIR/${mode_name}.log"
            echo "" >> "$BW_RESULTS_DIR/${mode_name}.log"
        done

        # Extract metrics
        grep "METRICS:" "$BW_RESULTS_DIR/${mode_name}.log" > "$BW_RESULTS_DIR/${mode_name}.metrics.json" 2>/dev/null || echo "No metrics found" > "$BW_RESULTS_DIR/${mode_name}.metrics.json"

        # Clear bandwidth constraints
        clear_bandwidth_constraints "$topology_name"

        # Destroy topology
        echo "  Destroying topology..."
        containerlab destroy -t "$topology_file" --cleanup > /dev/null 2>&1

        end_time=$(date +%s)
        duration=$((end_time - start_time))

        echo "  ✓ Test completed in ${duration}s"
        echo ""

        # Brief pause between tests
        sleep 5
    done

    echo "✓ Completed $bw_name tests"
    echo ""
done

# Generate summary report
echo "======================================"
echo "Generating Summary Report"
echo "======================================"

SUMMARY_FILE="$RESULTS_BASE_DIR/COMPREHENSIVE_SUMMARY.md"

cat > "$SUMMARY_FILE" <<EOF
# Traditional IoT Bandwidth Test Results

**Date:** $(date +%Y-%m-%d)
**Architecture:** Traditional IoT (NO CRDT, periodic full-state messaging)

## Test Configuration

- **Update Frequency:** 5 seconds
- **Message Type:** Full state (no deltas)
- **Sync Method:** Last-write-wins
- **Topologies:** Client-Server, Hub-Spoke
- **Bandwidth Levels:** 100Mbps, 10Mbps, 1Mbps, 256Kbps
- **Test Duration:** 60 seconds per test

## Results by Bandwidth

EOF

for bw_name in "100mbps" "10mbps" "1mbps" "256kbps"; do
    echo "" >> "$SUMMARY_FILE"
    echo "### $bw_name" >> "$SUMMARY_FILE"
    echo "" >> "$SUMMARY_FILE"

    for mode_config in "${MODES[@]}"; do
        mode_name="${mode_config%%:*}"

        if [ -f "$RESULTS_BASE_DIR/$bw_name/${mode_name}.metrics.json" ]; then
            msg_count=$(grep -c "MessageSent" "$RESULTS_BASE_DIR/$bw_name/${mode_name}.metrics.json" 2>/dev/null || echo "0")
            echo "- **$mode_name:** $msg_count messages sent" >> "$SUMMARY_FILE"
        fi
    done
done

cat >> "$SUMMARY_FILE" <<EOF

## Traditional IoT Characteristics

**Advantages:**
- Simple implementation
- Predictable behavior
- No CRDT overhead

**Disadvantages:**
- Periodic full-state transmission (inefficient)
- No automatic conflict resolution
- No convergence guarantees
- Fixed update frequency regardless of changes

## Files Generated

$(ls -lh "$RESULTS_BASE_DIR" | grep -v "^total" | awk '{print "- " $9 " (" $5 ")"}'  )

EOF

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Traditional IoT Bandwidth Testing Complete!             ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Results saved to: $RESULTS_BASE_DIR/"
echo ""
echo "View summary:"
echo "  cat $RESULTS_BASE_DIR/COMPREHENSIVE_SUMMARY.md"
echo ""
